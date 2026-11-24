//! apigen — generate service API interfaces + data models from OpenAPI/AsyncAPI specs.
//!
//! Pipeline: YAML specs -> [`loader`] -> typed [`ir::ApiIR`] -> [`emit`] (Quarkus Java and/or Rust).
//! Generation is a FULL REGENERATE: each target directory is wiped and rebuilt,
//! so the specs remain the single source of truth.

pub mod emit;
pub mod ir;
pub mod loader;
pub mod naming;
