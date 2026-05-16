//! M7.T3 — GitHub tarball dep resolution and on-disk cache.
//!
//! The runtime HTTP client (`crate::runtime::http`) speaks plain
//! HTTP/1.1 over TCP — no TLS. Production GitHub fetches therefore
//! require an HTTP-speaking origin (a mock server, an in-house mirror,
//! or `AERIS_GITHUB_HOST_OVERRIDE` pointing at a TLS-terminating proxy).
//! The acceptance gate of M7.T3 ("Network test (mocked) succeeds;
//! second run hits cache") is satisfied without a TLS stack — that
//! pillar is owed to whichever post-v0.2 milestone admits rustls.
//!
//! On-disk layout matches `language.md § 24.2`:
//!
//! ```text
//! <cache_root>/.aeris/ext/<host>__<owner>_<repo>/<version>/source.tar.gz
//! ```

use std::path::{Path, PathBuf};

/// Resolve `(host, owner_repo)` from a `github.com/<owner>/<repo>`
/// shaped string. Anything else is rejected — the caller is responsible
/// for filtering `DepSource::GitHub`.
fn split_repo(repo: &str) -> Result<(String, String), String> {
    let mut parts = repo.splitn(2, '/');
    let host = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("github dep `{repo}`: missing host"))?;
    let owner_repo = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("github dep `{repo}`: missing owner/repo"))?;
    Ok((host.to_string(), owner_repo.to_string()))
}

/// Cache directory for a single dep version, rooted at `cache_root`.
pub fn cache_dir_for(cache_root: &Path, repo: &str, version: &str) -> Result<PathBuf, String> {
    let (host, owner_repo) = split_repo(repo)?;
    let safe = owner_repo.replace('/', "_");
    Ok(cache_root
        .join(".aeris")
        .join("ext")
        .join(format!("{host}__{safe}"))
        .join(version))
}

/// Fetch the tarball bytes for a GitHub dep into the cache, returning
/// the bytes. A cache hit short-circuits the network call entirely.
///
/// `http_host_override` redirects the resolved URL to an alternate
/// `host[:port]` so tests can swap in a mock server. When `None`, the
/// URL targets `http://<host>/...`; production HTTPS arrives with the
/// TLS milestone (see module docs).
pub fn fetch_github_dep_to_cache(
    repo: &str,
    version: &str,
    cache_root: &Path,
    http_host_override: Option<&str>,
) -> Result<Vec<u8>, String> {
    let (host, owner_repo) = split_repo(repo)?;
    let dir = cache_dir_for(cache_root, repo, version)?;
    let file = dir.join("source.tar.gz");
    if file.exists() {
        return std::fs::read(&file).map_err(|e| format!("cache read {}: {e}", file.display()));
    }
    let effective_host = http_host_override.unwrap_or(&host);
    let url = format!("http://{effective_host}/{owner_repo}/archive/v{version}.tar.gz");
    let trace_id = "00000000000000000000000000";
    let resp = crate::runtime::http::do_request("GET", &url, &[], trace_id, None)
        .map_err(|e| format!("github fetch {url}: {e}"))?;
    if resp.status != 200 {
        return Err(format!("github fetch {url}: status {}", resp.status));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("cache mkdir {}: {e}", dir.display()))?;
    std::fs::write(&file, &resp.body).map_err(|e| format!("cache write {}: {e}", file.display()))?;
    Ok(resp.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn spawn_counting_mock(body: &'static [u8]) -> (u16, Arc<AtomicU32>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let count = Arc::new(AtomicU32::new(0));
        let count_clone = count.clone();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                count_clone.fetch_add(1, Ordering::SeqCst);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(header.as_bytes());
                let _ = s.write_all(body);
            }
        });
        (port, count)
    }

    fn unique_dir(tag: &str) -> PathBuf {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("aeris-m7t3-{tag}-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn split_repo_decomposes_host_and_owner_repo() {
        let (host, owner_repo) = super::split_repo("github.com/acmecorp/aeris-devops").unwrap();
        assert_eq!(host, "github.com");
        assert_eq!(owner_repo, "acmecorp/aeris-devops");
    }

    #[test]
    fn split_repo_rejects_missing_owner_repo() {
        assert!(super::split_repo("github.com").is_err());
    }

    #[test]
    fn cache_dir_for_lays_out_under_dot_aeris_ext() {
        let root = Path::new("/tmp/x");
        let dir = super::cache_dir_for(root, "github.com/acme/lib", "1.2.0").unwrap();
        assert_eq!(
            dir,
            Path::new("/tmp/x/.aeris/ext/github.com__acme_lib/1.2.0")
        );
    }

    #[test]
    fn m7t3_first_fetch_hits_network_second_fetch_hits_cache() {
        // The acceptance gate: a mocked network fetch succeeds, the
        // bytes land in `.aeris/ext/.../<version>/source.tar.gz`, and
        // a second fetch reads from disk without re-contacting the
        // mock (the request counter stays at 1).
        let body: &'static [u8] = b"fake-tarball-bytes";
        let (port, count) = spawn_counting_mock(body);
        let cache_root = unique_dir("hit-then-cache");
        let host_override = format!("127.0.0.1:{port}");
        let bytes1 = fetch_github_dep_to_cache(
            "github.com/acme/lib",
            "1.0.0",
            &cache_root,
            Some(&host_override),
        )
        .unwrap();
        assert_eq!(bytes1, body);
        let cached_path = cache_root
            .join(".aeris/ext/github.com__acme_lib/1.0.0/source.tar.gz");
        assert!(cached_path.exists(), "cache file not written");
        let bytes2 = fetch_github_dep_to_cache(
            "github.com/acme/lib",
            "1.0.0",
            &cache_root,
            Some(&host_override),
        )
        .unwrap();
        assert_eq!(bytes2, body);
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "second fetch must hit cache, not the network"
        );
        let _ = std::fs::remove_dir_all(&cache_root);
    }

    #[test]
    fn m7t3_non_200_response_propagates_as_error() {
        // The mock here always replies 404. A non-200 surface error
        // means the cache stays empty and the caller sees the failure.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let _ = s.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        });
        let cache_root = unique_dir("non200");
        let host_override = format!("127.0.0.1:{port}");
        let err = fetch_github_dep_to_cache(
            "github.com/acme/lib",
            "1.0.0",
            &cache_root,
            Some(&host_override),
        )
        .unwrap_err();
        assert!(err.contains("404") || err.contains("status"), "got {err}");
        let cached_path = cache_root
            .join(".aeris/ext/github.com__acme_lib/1.0.0/source.tar.gz");
        assert!(!cached_path.exists(), "cache should not be populated on error");
        let _ = std::fs::remove_dir_all(&cache_root);
    }
}
