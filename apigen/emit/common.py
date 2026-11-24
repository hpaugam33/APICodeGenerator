"""Shared helpers for emitters."""

import os


def generated_header(lang, command):
    if lang == "java":
        return f"// GENERATED CODE — DO NOT EDIT.\n// Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:\n//   {command}"
    if lang == "rust":
        return f"//! GENERATED CODE — DO NOT EDIT.\n//! Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:\n//!   {command}"
    return f"# GENERATED CODE — DO NOT EDIT. Regenerate with:\n#   {command}"


def write_file(root, relpath, content):
    path = os.path.join(root, relpath)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(content if content.endswith("\n") else content + "\n")
    return path


def common_path_prefix(paths):
    """Longest common leading segments of a list of OpenAPI paths -> base @Path."""
    split = [p.strip("/").split("/") for p in paths]
    if not split:
        return ""
    prefix = []
    for segs in zip(*split):
        if len(set(segs)) == 1:
            prefix.append(segs[0])
        else:
            break
    return "/" + "/".join(prefix) if prefix else ""


def indent(text, level=1):
    pad = "    " * level
    return "\n".join(pad + line if line else line for line in text.splitlines())
