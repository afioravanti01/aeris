//! Aeris lockset: `lockset.toml` parsing, blake3 content-addressing,
//! `surface.lock` writer/reader, `main` cap synthesis.
//!
//! Realises `docs/language.md` § 24 and the V3 / N4 patches.

pub mod fetch;
#[allow(clippy::module_inception)]
pub mod lockset;
pub mod surface;
pub mod toml;

pub use lockset::{
    parse_lockset, verify_local_deps, AiBackend, CapsCeiling, DepEntry, DepSource, Lockset,
    LocksetError, ProjectInfo, EXIT_LOCKSET_ERROR,
};
pub use surface::{
    compute_dep_surface_hash, compute_surface, diff_surface_bodies, hash_text, write_surface_lock,
    SurfaceEntry, SurfaceLock,
};
