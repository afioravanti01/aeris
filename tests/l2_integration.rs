//! End-to-end smoke test for the M22 real L2 backends.
//!
//! The FS-backed variants (T4/T5/T8) run without any external
//! service, so the happy-path test always executes. SDK-backed
//! variants (T4-bis / T5-bis / T8-bis) and the subprocess-backed
//! docker / kube real paths (T6/T7) are gated on `AERIS_INT_*=1`
//! environment variables — un-set variables skip the test.

use aeris::runtime::l2_backend::{L2Backends, MinioBackend, RealMinio};
use aeris::runtime::l2_runtime;
use aeris::runtime::Env;
use aeris::runtime::Value;
use aeris::manifest::{BackendKind, L2BackendsConfig, MinioBackendConfig};

fn tmp_dir(label: &str) -> std::path::PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("aeris-l2-int-{label}-{pid}-{nanos}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn real_minio_fs_put_get_round_trips() {
    let root = tmp_dir("minio-put-get");
    let backend = RealMinio::new(
        Some(format!("file://{}", root.display())),
        l2_runtime::shared(),
    );
    let env = Env::new();
    let span = aeris::syntax::token::Span::ZERO;

    // mb → put → get → list.
    backend.mb(&env, "kb", span).unwrap();
    backend
        .put(&env, "kb", "a.md", &Value::Str("hello".into()), span)
        .unwrap();
    let got = backend.get(&env, "kb", "a.md", span).unwrap();
    match got {
        Value::Result(Ok(inner)) => match *inner {
            Value::Bytes(b) => assert_eq!(b, b"hello"),
            other => panic!("expected Bytes, got {other:?}"),
        },
        other => panic!("expected Ok(Bytes), got {other:?}"),
    }
}

#[test]
fn from_manifest_wires_real_minio_when_backend_real() {
    // A bare `L2BackendsConfig` with `minio.backend = Real` and a
    // `file://` endpoint must yield a backend table whose
    // `minio.put` writes a real file. The integration here is
    // the `from_manifest` builder, not the SDK.
    let root = tmp_dir("manifest-wire");
    let cfg = L2BackendsConfig {
        minio: MinioBackendConfig {
            backend: BackendKind::Real,
            endpoint: Some(format!("file://{}", root.display())),
            ..Default::default()
        },
        ..Default::default()
    };
    let backends = L2Backends::from_manifest(&cfg, l2_runtime::shared());
    let env = Env::new();
    let span = aeris::syntax::token::Span::ZERO;
    backends
        .minio
        .put(&env, "demo", "x", &Value::Str("y".into()), span)
        .unwrap();
    assert!(root.join("demo").join("x").is_file());
}
