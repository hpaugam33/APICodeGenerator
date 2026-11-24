//! Language emitters. Each takes the ApiIR and writes a full target tree into
//! the output directory, returning the written file paths.

pub mod common;
pub mod quarkus;
pub mod rust;

/// Emitter names in sorted order, as used by `--target both`.
pub const EMITTERS: [&str; 2] = ["quarkus", "rust"];
