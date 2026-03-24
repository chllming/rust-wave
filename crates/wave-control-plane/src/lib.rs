//! Compatibility shim for the reducer-backed projection surface.
//!
//! `wave-projections` now owns the human-facing planning, queue, and control
//! read models, operator status helpers, and the operator snapshot input
//! spine. Prefer depending on `wave-projections` directly for new work; this
//! crate remains a forwarding layer so existing runtime and CLI callers keep
//! compiling while the workspace finishes the manifest cutover. It does not
//! contain separate reducer or projection authority under the older crate
//! name.

pub use wave_projections::*;
