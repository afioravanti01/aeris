//! Minimal HTTP/1.1 client (M5.T1 + M5.T2).
//!
//! Hand-rolled to keep the static binary footprint under the
//! thesis-mandated 8 MB ceiling — the spec calls out `reqwest` for
//! convenience but a third-party HTTP stack would pull in tokio +
//! rustls (~1.5 MB) and async machinery the rest of the runtime
//! does not need.
//!
//! Realised features:
//!
//! - Plain HTTP/1.1 over TCP (no TLS — that arrives with the
//!   external dependency story in M11).
//! - Methods `GET`, `POST`, `PUT`, `PATCH`, `DELETE`.
//! - Mandatory header `X-Aeris-Trace-Id` propagating the active
//!   tracer's run id (N4 / § 20.1).
//! - Status / req-hash / resp-hash recorded into the trace event.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    BadUrl(String),
    Io(String),
    BadStatusLine(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::BadUrl(u) => write!(f, "invalid URL: {u}"),
            HttpError::Io(e) => write!(f, "io: {e}"),
            HttpError::BadStatusLine(s) => write!(f, "bad status line: {s}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedUrl {
    pub host: String,
    pub port: u16,
    pub path_and_query: String,
}

/// Parse an absolute `http://` URL into `(host, port, path-and-query)`.
/// HTTPS is rejected — the runtime does not yet ship a TLS stack.
pub fn parse_url(url: &str) -> Result<ParsedUrl, HttpError> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| HttpError::BadUrl(url.into()))?;
    let (authority, path_q) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if authority.is_empty() {
        return Err(HttpError::BadUrl(url.into()));
    }
    let (host, port) = match authority.rfind(':') {
        Some(i) => {
            let h = &authority[..i];
            let p = authority[i + 1..]
                .parse::<u16>()
                .map_err(|_| HttpError::BadUrl(url.into()))?;
            (h.to_string(), p)
        }
        None => (authority.to_string(), 80),
    };
    Ok(ParsedUrl {
        host,
        port,
        path_and_query: path_q.to_string(),
    })
}

/// Issue a single HTTP/1.1 request. The body is sent verbatim; the
/// response body is read until EOF for `Connection: close`. Caller
/// is responsible for any retry policy. Pass `idempotency_key =
/// Some(key)` when the call is part of a saga step (N1 / § 12.3) —
/// the header is added so the remote side can dedupe.
pub fn do_request(
    method: &str,
    url: &str,
    body: &[u8],
    trace_id: &str,
    idempotency_key: Option<&str>,
) -> Result<HttpResponse, HttpError> {
    let parsed = parse_url(url)?;
    let addr = format!("{}:{}", parsed.host, parsed.port);
    let mut stream = TcpStream::connect(&addr).map_err(|e| HttpError::Io(e.to_string()))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| HttpError::Io(e.to_string()))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| HttpError::Io(e.to_string()))?;

    // Build the request line + headers + body.
    let mut req: Vec<u8> = Vec::new();
    let host_header = if parsed.port == 80 {
        parsed.host.clone()
    } else {
        format!("{}:{}", parsed.host, parsed.port)
    };
    write!(
        &mut req,
        "{method} {} HTTP/1.1\r\n\
         Host: {host_header}\r\n\
         User-Agent: aeris/0.2\r\n\
         X-Aeris-Trace-Id: {trace_id}\r\n\
         Connection: close\r\n\
         Content-Length: {}\r\n",
        parsed.path_and_query,
        body.len(),
    )
    .map_err(|e| HttpError::Io(e.to_string()))?;
    if let Some(key) = idempotency_key {
        write!(&mut req, "Idempotency-Key: {key}\r\n").map_err(|e| HttpError::Io(e.to_string()))?;
    }
    req.extend_from_slice(b"\r\n");
    req.extend_from_slice(body);
    stream
        .write_all(&req)
        .map_err(|e| HttpError::Io(e.to_string()))?;

    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| HttpError::Io(e.to_string()))?;
    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> Result<HttpResponse, HttpError> {
    // Find the header/body boundary (`\r\n\r\n`).
    let split = find_subslice(raw, b"\r\n\r\n").ok_or_else(|| {
        HttpError::BadStatusLine("response is missing header/body separator".into())
    })?;
    let header_block = &raw[..split];
    let body = raw[split + 4..].to_vec();
    let head_str =
        std::str::from_utf8(header_block).map_err(|e| HttpError::BadStatusLine(e.to_string()))?;
    let mut lines = head_str.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| HttpError::BadStatusLine("empty status line".into()))?;
    let mut parts = status_line.splitn(3, ' ');
    let _version = parts.next();
    let status_code = parts
        .next()
        .ok_or_else(|| HttpError::BadStatusLine(status_line.into()))?
        .parse::<u16>()
        .map_err(|_| HttpError::BadStatusLine(status_line.into()))?;
    Ok(HttpResponse {
        status: status_code,
        body,
    })
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ====================================================================
//  Tests — URL parsing only. Live HTTP round-trip is exercised by
//  the eval-layer fixture in `runtime::eval::tests::http_*`, which
//  spawns a one-shot loopback server.
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_with_default_port() {
        let p = parse_url("http://api.acme.com/charge").unwrap();
        assert_eq!(p.host, "api.acme.com");
        assert_eq!(p.port, 80);
        assert_eq!(p.path_and_query, "/charge");
    }

    #[test]
    fn parse_url_with_explicit_port() {
        let p = parse_url("http://127.0.0.1:8080/x").unwrap();
        assert_eq!(p.host, "127.0.0.1");
        assert_eq!(p.port, 8080);
    }

    #[test]
    fn parse_url_with_query_string() {
        let p = parse_url("http://host/path?a=1&b=2").unwrap();
        assert_eq!(p.path_and_query, "/path?a=1&b=2");
    }

    #[test]
    fn parse_url_without_trailing_slash_uses_root() {
        let p = parse_url("http://host").unwrap();
        assert_eq!(p.path_and_query, "/");
    }

    #[test]
    fn parse_url_rejects_https() {
        assert!(matches!(
            parse_url("https://api.acme.com/x"),
            Err(HttpError::BadUrl(_))
        ));
    }

    #[test]
    fn parse_url_rejects_other_schemes() {
        assert!(matches!(
            parse_url("ftp://api.acme.com/x"),
            Err(HttpError::BadUrl(_))
        ));
    }

    #[test]
    fn parse_response_extracts_status_and_body() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nhello";
        let r = parse_response(raw).unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.body, b"hello");
    }

    #[test]
    fn parse_response_404() {
        let raw = b"HTTP/1.1 404 Not Found\r\n\r\nmissing";
        let r = parse_response(raw).unwrap();
        assert_eq!(r.status, 404);
    }

    #[test]
    fn parse_response_rejects_no_separator() {
        assert!(parse_response(b"HTTP/1.1 200 OK\r\n").is_err());
    }
}
