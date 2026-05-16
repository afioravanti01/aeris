//! M20 — minimal HTTP server runtime.
//!
//! Implements `net.http(port) -> http_server` + `server.accept() ->
//! http_req` + `req.reply(status, body, content_type)`. The
//! implementation is *deliberately small*: a single-threaded
//! blocking TCP listener, one HTTP/1.1 request parsed per accept,
//! plain-text response written back on the same connection.
//!
//! Concurrency is the caller's job: wrap each `server.accept()` in
//! `spawn { … }` and the OS scheduler will multiplex (project.md
//! §19.1). There is no built-in pool.
//!
//! Listeners and live request streams are stored in thread-local
//! registries keyed by integer handles, because Aeris `Value`s are
//! plain enums and we want the user-facing types to stay simple.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

thread_local! {
    static SERVERS: RefCell<HashMap<i64, TcpListener>> = RefCell::new(HashMap::new());
    static STREAMS: RefCell<HashMap<i64, TcpStream>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn next_id() -> i64 {
    NEXT_ID.with(|c| {
        let mut id = c.borrow_mut();
        let v = *id;
        *id += 1;
        v
    })
}

/// Bind a TCP listener to the given port; return the server handle.
pub fn http_serve(port: u16) -> Result<i64, String> {
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).map_err(|e| format!("net.http({port}): {e}"))?;
    let id = next_id();
    SERVERS.with(|m| {
        m.borrow_mut().insert(id, listener);
    });
    Ok(id)
}

/// Block until the next connection. Returns `(conn_id, request)`
/// where `request` is a flat record with keys
/// `method, path, query_raw, headers, body, remote_addr`.
pub fn http_accept(server_id: i64) -> Result<AcceptedReq, String> {
    let (stream, addr) = SERVERS.with(|m| -> Result<_, String> {
        let map = m.borrow();
        let listener = map
            .get(&server_id)
            .ok_or_else(|| format!("net.http: unknown server id {server_id}"))?;
        listener.accept().map_err(|e| format!("net.http accept: {e}"))
    })?;
    let conn_id = next_id();
    // Clone the stream so we can stash one half in STREAMS and use
    // the other half to read the request line + headers + body.
    let read_stream = stream.try_clone().map_err(|e| format!("net.http accept: clone: {e}"))?;
    STREAMS.with(|m| {
        m.borrow_mut().insert(conn_id, stream);
    });
    let req = parse_http_request(read_stream)?;
    Ok(AcceptedReq {
        conn_id,
        method: req.method,
        path: req.path,
        query_raw: req.query_raw,
        headers: req.headers,
        body: req.body,
        remote_addr: addr.to_string(),
    })
}

/// Write the response back to the captured connection and remove
/// it from the registry. Idempotent — a second call on the same id
/// is a no-op.
pub fn http_reply(conn_id: i64, status: u16, body: &str, content_type: &str) -> Result<(), String> {
    let mut stream = STREAMS.with(|m| m.borrow_mut().remove(&conn_id));
    let Some(s) = stream.as_mut() else {
        return Ok(());
    };
    let reason = http_reason(status);
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.as_bytes().len(),
        body,
    );
    s.write_all(response.as_bytes())
        .map_err(|e| format!("net.http reply: {e}"))?;
    s.flush().ok();
    Ok(())
}

pub struct AcceptedReq {
    pub conn_id: i64,
    pub method: String,
    pub path: String,
    pub query_raw: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub remote_addr: String,
}

struct ParsedReq {
    method: String,
    path: String,
    query_raw: String,
    headers: Vec<(String, String)>,
    body: String,
}

fn parse_http_request(stream: TcpStream) -> Result<ParsedReq, String> {
    let mut r = BufReader::new(stream);
    let mut line = String::new();
    r.read_line(&mut line).map_err(|e| format!("read request line: {e}"))?;
    let parts: Vec<&str> = line.trim_end_matches(['\r', '\n']).split_whitespace().collect();
    if parts.len() < 2 {
        return Err(format!("malformed request line: {line:?}"));
    }
    let method = parts[0].to_string();
    let target = parts[1].to_string();
    let (path, query_raw) = match target.find('?') {
        Some(i) => (target[..i].to_string(), target[i + 1..].to_string()),
        None => (target, String::new()),
    };
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut content_length: usize = 0;
    loop {
        let mut header_line = String::new();
        let n = r.read_line(&mut header_line).map_err(|e| format!("read header: {e}"))?;
        if n == 0 {
            break;
        }
        let trimmed = header_line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(colon) = trimmed.find(':') {
            let k = trimmed[..colon].trim().to_string();
            let v = trimmed[colon + 1..].trim().to_string();
            if k.eq_ignore_ascii_case("content-length") {
                if let Ok(n) = v.parse::<usize>() {
                    content_length = n;
                }
            }
            headers.push((k, v));
        }
    }
    let mut body_bytes = vec![0u8; content_length];
    if content_length > 0 {
        r.read_exact(&mut body_bytes)
            .map_err(|e| format!("read body: {e}"))?;
    }
    let body = String::from_utf8_lossy(&body_bytes).into_owned();
    Ok(ParsedReq {
        method,
        path,
        query_raw,
        headers,
        body,
    })
}

fn http_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "OK",
    }
}
