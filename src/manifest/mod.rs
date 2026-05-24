//! Aeris manifest: `aeris.toml` parsing, blake3 content-addressing,
//! `surface.lock` writer/reader, `main` cap synthesis.
//!
//! Realises `docs/language.md` § 24 and the V3 / N4 patches.

pub mod fetch;
#[allow(clippy::module_inception)]
pub mod manifest;
pub mod surface;
pub mod toml;

pub use manifest::{
    parse_manifest, verify_local_deps, AiBackend, AuditBackendConfig, BackendKind, CapsCeiling,
    DepEntry, DepSource, DockerBackendConfig, EnforceMode, KubeBackendConfig, L2BackendsConfig,
    Manifest, ManifestError, MinioBackendConfig, ModuleEntry, MongoBackendConfig, ProjectInfo,
    RabbitBackendConfig, RuntimeConfig, EXIT_MANIFEST_ERROR,
};
pub use surface::{
    compute_dep_surface_hash, compute_surface, diff_surface_bodies, hash_text, write_surface_lock,
    SurfaceEntry, SurfaceLock,
};
