//! Shared helpers for emitters.

use std::path::{Path, PathBuf};

use anyhow::Result;

pub fn generated_header(lang: &str, command: &str) -> String {
    match lang {
        "java" => format!(
            "// GENERATED CODE — DO NOT EDIT.\n// Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:\n//   {command}"
        ),
        "rust" => format!(
            "//! GENERATED CODE — DO NOT EDIT.\n//! Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:\n//!   {command}"
        ),
        _ => format!("# GENERATED CODE — DO NOT EDIT. Regenerate with:\n#   {command}"),
    }
}

pub fn write_file(root: &Path, relpath: &str, content: &str) -> Result<PathBuf> {
    let path = root.join(relpath);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut c = content.to_string();
    if !c.ends_with('\n') {
        c.push('\n');
    }
    std::fs::write(&path, c)?;
    Ok(path)
}

/// Longest common leading segments of a list of OpenAPI paths -> base @Path.
pub fn common_path_prefix(paths: &[String]) -> String {
    let split: Vec<Vec<&str>> = paths
        .iter()
        .map(|p| p.trim_matches('/').split('/').collect())
        .collect();
    if split.is_empty() {
        return String::new();
    }
    let mut prefix: Vec<&str> = vec![];
    for i in 0..split[0].len() {
        let seg = split[0][i];
        if split.iter().all(|segs| segs.get(i) == Some(&seg)) {
            prefix.push(seg);
        } else {
            break;
        }
    }
    if prefix.is_empty() { String::new() } else { format!("/{}", prefix.join("/")) }
}
