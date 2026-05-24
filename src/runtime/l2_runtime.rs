//! Async ↔ sync bridge for L2 real backends (M22.T3).
//!
//! The tree-walk interpreter is synchronous; real SDKs (S3,
//! MongoDB, Kubernetes, AMQP, Docker over HTTP) are async. An
//! `L2Runtime` owns a single-threaded Tokio runtime and exposes
//! `block_on` so each call from a `Real*Backend` can `await` its
//! future on the same OS thread the interpreter runs on. That
//! sidesteps the `!Send` constraint on `Env` (it stores
//! `Rc<RefCell<…>>`) without forcing any per-call thread
//! migration.
//!
//! The runtime is created lazily on first use: a program that
//! never touches a real backend pays nothing (no tokio
//! reactor / timer threads, no allocations beyond a stub `Rc`).
//!
//! Errors are surfaced as `EvalErrorKind::Raised(Value::Str(…))`
//! tagged with a known prefix so user code can pattern-match on
//! the failure kind (`err.io.network`, `err.io.auth`, …). The
//! prefix list is closed: `err.io.network` for transport-level
//! failures, `err.io.auth` for 401/403/credential issues,
//! `err.io.not_found` for 404, `err.io.timeout` for deadline
//! overruns, and `err.io` for anything else (the inner message
//! always carries the original SDK error so diagnostics stay
//! recoverable).

use std::cell::RefCell;
use std::rc::Rc;

use super::eval::{EvalError, EvalErrorKind};
use super::value::Value;
use crate::syntax::token::Span;

/// Owns the Tokio current-thread runtime. Cloning is cheap (the
/// runtime itself is shared by `Rc`); construction happens at
/// `L2Runtime::new()` and panics only if Tokio itself can't bring
/// up a current-thread reactor — a hard runtime invariant.
pub struct L2Runtime {
    inner: RefCell<Option<tokio::runtime::Runtime>>,
}

impl L2Runtime {
    /// Build a fresh runtime. Real backends take an
    /// `Rc<L2Runtime>` and share the same reactor across calls.
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(None),
        }
    }

    /// Block on a future. Drives it to completion on the owned
    /// current-thread runtime. The runtime is built on first call
    /// so callers that never reach a real backend don't pay for
    /// it. `F::Output` is returned verbatim — error mapping into
    /// `EvalError` is up to the caller (see
    /// [`sdk_error_to_raised`]).
    pub fn block_on<F>(&self, fut: F) -> F::Output
    where
        F: std::future::Future,
    {
        let mut slot = self.inner.borrow_mut();
        if slot.is_none() {
            *slot = Some(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("L2Runtime: failed to start Tokio current-thread runtime"),
            );
        }
        slot.as_ref().unwrap().block_on(fut)
    }
}

impl Default for L2Runtime {
    fn default() -> Self {
        Self::new()
    }
}

/// Classify an SDK error message into the closed `err.io.*`
/// taxonomy and wrap it in `Raised(Value::Str(…))` so the user's
/// `?` propagates it. The classifier is intentionally string-based
/// — every SDK exposes a stable enough textual error surface, and
/// the kind prefix is the part user code matches against. The
/// full original message is preserved after the prefix so
/// diagnostics never lose information.
pub fn sdk_error_to_raised(family: &str, op: &str, msg: &str, span: Span) -> EvalError {
    let lc = msg.to_ascii_lowercase();
    let kind = if lc.contains("401")
        || lc.contains("403")
        || lc.contains("unauthorized")
        || lc.contains("forbidden")
        || lc.contains("auth")
        || lc.contains("credential")
    {
        "err.io.auth"
    } else if lc.contains("404") || lc.contains("not found") || lc.contains("nosuchkey") {
        "err.io.not_found"
    } else if lc.contains("timeout") || lc.contains("timed out") || lc.contains("deadline") {
        "err.io.timeout"
    } else if lc.contains("connection")
        || lc.contains("dial")
        || lc.contains("network")
        || lc.contains("dns")
        || lc.contains("refused")
        || lc.contains("reset")
    {
        "err.io.network"
    } else {
        "err.io"
    };
    EvalError::new(
        EvalErrorKind::Raised(Value::Str(format!("{kind}: {family}.{op}: {msg}"))),
        span,
    )
}

/// Cheap accessor used by the `Env`-level builder so callers can
/// share one runtime across every backend without re-instantiating.
pub fn shared() -> Rc<L2Runtime> {
    Rc::new(L2Runtime::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn l2_runtime_block_on_awaits_future() {
        let rt = L2Runtime::new();
        let v: i32 = rt.block_on(async {
            tokio::time::sleep(Duration::from_millis(1)).await;
            42
        });
        assert_eq!(v, 42);
    }

    #[test]
    fn l2_runtime_runtime_is_reusable_across_calls() {
        let rt = L2Runtime::new();
        let a = rt.block_on(async { 1 + 2 });
        let b = rt.block_on(async { 4 * 5 });
        assert_eq!(a, 3);
        assert_eq!(b, 20);
    }

    #[test]
    fn sdk_error_network_kind() {
        let e = sdk_error_to_raised(
            "minio",
            "put",
            "connection refused while dialing endpoint",
            Span::ZERO,
        );
        match e.kind {
            EvalErrorKind::Raised(Value::Str(s)) => {
                assert!(s.starts_with("err.io.network:"), "{s}");
                assert!(s.contains("minio.put"));
                assert!(s.contains("connection refused"));
            }
            other => panic!("expected Raised(Str), got {other:?}"),
        }
    }

    #[test]
    fn sdk_error_auth_kind() {
        let e = sdk_error_to_raised("minio", "put", "HTTP 403 forbidden", Span::ZERO);
        match e.kind {
            EvalErrorKind::Raised(Value::Str(s)) => assert!(s.starts_with("err.io.auth:"), "{s}"),
            other => panic!("expected Raised(Str), got {other:?}"),
        }
    }

    #[test]
    fn sdk_error_not_found_kind() {
        let e = sdk_error_to_raised("minio", "get", "NoSuchKey: object does not exist", Span::ZERO);
        match e.kind {
            EvalErrorKind::Raised(Value::Str(s)) => {
                assert!(s.starts_with("err.io.not_found:"), "{s}");
            }
            other => panic!("expected Raised(Str), got {other:?}"),
        }
    }

    #[test]
    fn sdk_error_timeout_kind() {
        let e = sdk_error_to_raised("kube", "apply", "operation timed out after 30s", Span::ZERO);
        match e.kind {
            EvalErrorKind::Raised(Value::Str(s)) => {
                assert!(s.starts_with("err.io.timeout:"), "{s}");
            }
            other => panic!("expected Raised(Str), got {other:?}"),
        }
    }

    #[test]
    fn sdk_error_falls_back_to_generic_io_kind() {
        let e = sdk_error_to_raised("mongo", "write", "unknown failure flavor", Span::ZERO);
        match e.kind {
            EvalErrorKind::Raised(Value::Str(s)) => {
                assert!(s.starts_with("err.io:"), "{s}");
                assert!(!s.starts_with("err.io."), "{s}"); // not one of the prefixed kinds
            }
            other => panic!("expected Raised(Str), got {other:?}"),
        }
    }
}
