//! Atrius core data model.
//!
//! This crate encodes the file-centric entities, states, and invariants
//! described in `docs/data-model.md`. It is intentionally light on behavior:
//! just enough structure to enforce invariants and support future engine code.

pub mod model;

pub use model::*;
