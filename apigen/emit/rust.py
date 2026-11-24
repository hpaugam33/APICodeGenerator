"""Emit a Rust service skeleton from the ApiIR.

Layout (fully regenerated on each run — spec is the source of truth):
    Cargo.toml
    src/main.rs            (router wiring)
    src/api/mod.rs         (ApiError + module decls)
    src/api/<tag>.rs       (axum handlers + router, one per OpenAPI tag)
    src/model/mod.rs       (re-exports)
    src/model/<name>.rs    (serde structs/enums)
    src/messaging/mod.rs   (async traits per AsyncAPI channel, broker-agnostic)

The messaging layer is generated as traits so the same interface works with
any broker client (lapin, rdkafka, ...); only the impl is hand-written.
"""

import os

from ..naming import camel, pascal, snake
from .common import generated_header, write_file

_RUST_KEYWORDS = {
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
    "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
    "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do",
    "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
}


def _rname(name):
    n = snake(name)
    return n + "_" if n in _RUST_KEYWORDS else n


def rust_type(tref, imports):
    """Map a TypeRef to a Rust type; register needed imports in `imports`."""
    k = tref.kind
    if k == "ref":
        return tref.ref
    if k == "string":
        f = tref.format
        if f == "date":
            imports.add("chrono::NaiveDate")
            return "NaiveDate"
        if f == "date-time":
            imports.add("chrono::DateTime")
            imports.add("chrono::Utc")
            return "DateTime<Utc>"
        if f == "uuid":
            imports.add("uuid::Uuid")
            return "Uuid"
        if f in ("byte", "binary"):
            return "Vec<u8>"
        return "String"
    if k == "integer":
        return "i32" if tref.format == "int32" else "i64"
    if k == "number":
        return "f32" if tref.format == "float" else "f64"
    if k == "boolean":
        return "bool"
    if k == "array":
        return "Vec<%s>" % rust_type(tref.item, imports)
    if k == "map":
        imports.add("std::collections::HashMap")
        v = rust_type(tref.value, imports)
        return "HashMap<String, %s>" % v
    if k == "object":
        imports.add("std::collections::HashMap")
        imports.add("serde_json::Value")
        return "HashMap<String, Value>"
    imports.add("serde_json::Value")
    return "Value"


def emit_model_file(model, command):
    imports = {"serde::Deserialize", "serde::Serialize"}
    lines = [generated_header("rust", command), ""]
    if model.kind == "enum":
        variants = []
        for v in model.enum_values:
            pv = pascal(v)
            if pv not in variants:
                variants.append(pv)
        body = ["#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]",
                '#[serde(rename_all = "SCREAMING_SNAKE_CASE")]']
        if model.description:
            body.insert(0, "/// %s" % model.description.replace("\n", " "))
        body.append("pub enum %s {" % model.name)
        body.append("    " + ",\n    ".join(variants) + ",")
        body.append("}")
        for imp in sorted(imports):
            lines.append("use %s;" % imp)
        lines.append("")
        lines.extend(body)
        return "\n".join(lines) + "\n"
    fields = []
    has_refs = False
    for f in model.fields:
        rtype = rust_type(f.type, imports)
        t = f.type
        while t is not None:
            if t.is_ref():
                has_refs = True
            t = t.item or t.value
        if not f.required:
            rtype = "Option<%s>" % rtype
            fields.append('    #[serde(default, skip_serializing_if = "Option::is_none")]')
        fields.append("    pub %s: %s," % (_rname(f.name), rtype))
    if has_refs:
        imports.add("super::*")
    body = ['#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]',
            '#[serde(rename_all = "camelCase")]']
    if model.description:
        body.insert(0, "/// %s" % model.description.replace("\n", " "))
    body.append("pub struct %s {" % model.name)
    body.extend(fields)
    body.append("}")
    for imp in sorted(imports):
        lines.append("use %s;" % imp)
    lines.append("")
    lines.extend(body)
    return "\n".join(lines) + "\n"


def _route_path(path):
    # axum 0.8 path syntax is already {param}; nothing to convert
    return path


def emit_api_module(tag, endpoints, command):
    imports = {"axum::extract::{Path, Query}", "axum::routing", "axum::{Json, Router}",
               "serde::Deserialize", "crate::model::*", "super::ApiError"}
    handlers, structs = [], []
    routes = {}
    for ep in sorted(endpoints, key=lambda e: (e.path, e.method)):
        handler = snake(ep.operation_id)
        path_params = [p for p in ep.params if p.location == "path"]
        query_params = [p for p in ep.params if p.location == "query"]
        args, pre = [], []
        if path_params:
            sname = pascal(ep.operation_id) + "Path"
            fields = []
            for p in path_params:
                t = rust_type(p.type, imports)
                fields.append('    #[serde(rename = "%s")]\n    pub %s: %s,' % (p.name, _rname(p.name), t))
            structs.append("#[derive(Debug, Deserialize)]\npub struct %s {\n%s\n}\n" % (sname, "\n".join(fields)))
            args.append("Path(path_params): Path<%s>" % sname)
        if query_params:
            sname = pascal(ep.operation_id) + "Query"
            fields = []
            for p in query_params:
                t = rust_type(p.type, imports)
                if not p.required:
                    t = "Option<%s>" % t
                fields.append("    pub %s: %s," % (_rname(p.name), t))
            structs.append("#[derive(Debug, Deserialize)]\npub struct %s {\n%s\n}\n" % (sname, "\n".join(fields)))
            args.append("Query(query_params): Query<%s>" % sname)
        if ep.request_body is not None:
            args.append("Json(body): Json<%s>" % rust_type(ep.request_body, imports))
        if ep.response is None:
            imports.add("axum::http::StatusCode")
            ret = "Result<StatusCode, ApiError>"
        else:
            ret = "Result<Json<%s>, ApiError>" % rust_type(ep.response, imports)
        body = '    todo!("implement %s")' % ep.operation_id
        doc = "/// %s" % ep.summary.replace("\n", " ") if ep.summary else ""
        handlers.append("%s\npub async fn %s(%s) -> %s {\n%s\n}" % (
            doc, handler, ", ".join(args), ret, body))
        routes.setdefault(ep.path, []).append((ep.method.lower(), handler))
    method_order = ["get", "post", "put", "patch", "delete", "head", "options"]
    route_lines = []
    for path, methods in sorted(routes.items()):
        methods = sorted(methods, key=lambda mh: method_order.index(mh[0])
                         if mh[0] in method_order else 99)
        chain = ".".join("%s(%s)" % (m, h) for m, h in methods)
        route_lines.append('        .route("%s", routing::%s)' % (_route_path(path), chain))
    lines = [generated_header("rust", command), ""]
    for imp in sorted(imports, key=lambda i: (0 if i.startswith(("crate::", "super::", "self::")) else 1, i)):
        lines.append("use %s;" % imp)
    lines.append("")
    lines.extend(structs)
    if structs:
        lines.append("")
    lines.extend(handlers)
    lines.append("")
    lines.append("pub fn router() -> Router {")
    lines.append("    Router::new()")
    lines.extend(route_lines)
    lines.append("}")
    return "\n".join(lines) + "\n"


def emit_api_mod(tags, command):
    lines = [generated_header("rust", command), ""]
    for tag in tags:
        lines.append("pub mod %s;" % snake(tag))
    if tags:
        lines.append("")
    lines += [
        "use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};",
        "use serde::Serialize;",
        "",
        "#[derive(Debug, Serialize)]",
        "pub struct ErrorBody {",
        "    pub code: String,",
        "    pub message: String,",
        "}",
        "",
        "#[derive(Debug)]",
        "pub enum ApiError {",
        "    NotFound(String),",
        "    NotImplemented,",
        "    Internal(String),",
        "}",
        "",
        "impl IntoResponse for ApiError {",
        "    fn into_response(self) -> Response {",
        "        let (status, code, message) = match self {",
        "            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, \"NOT_FOUND\", m),",
        "            ApiError::NotImplemented => (StatusCode::NOT_IMPLEMENTED, \"NOT_IMPLEMENTED\", \"handler not implemented\".to_string()),",
        "            ApiError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, \"INTERNAL\", m),",
        "        };",
        "        (status, Json(ErrorBody { code: code.to_string(), message })).into_response()",
        "    }",
        "}",
    ]
    return "\n".join(lines) + "\n"


def emit_main(tags, command):
    lines = [generated_header("rust", command), "",
             "mod api;", "mod messaging;", "mod model;", "",
             "use axum::Router;", "",
             "#[tokio::main]",
             "async fn main() {"]
    if tags:
        lines.append("    let app = Router::new()")
        for i, tag in enumerate(tags):
            suffix = ";" if i == len(tags) - 1 else ""
            lines.append("        .merge(api::%s::router())%s" % (snake(tag), suffix))
    else:
        lines.append("    let app = Router::new();")
    lines += ['    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")',
              '        .await',
              '        .expect("bind 127.0.0.1:8080");',
              '    axum::serve(listener, app).await.expect("serve");',
              "}"]
    return "\n".join(lines) + "\n"


def emit_messaging_mod(channels, command):
    lines = [generated_header("rust", command), ""]
    if not channels:
        lines.append("// No AsyncAPI flows in the input specs.")
        return "\n".join(lines) + "\n"
    lines += ["use async_trait::async_trait;", "use crate::model::*;", ""]
    for channel, info in sorted(channels.items()):
        if info["send"]:
            lines.append("/// Producer for channel `%s` (topic: %s) — implement with your broker client."
                         % (channel, info["topic"] or channel))
            lines.append("#[async_trait]")
            lines.append("pub trait %sProducer {" % pascal(channel))
            for f in info["send"]:
                ptype = rust_type(f.payload, set()) if f.payload else "String"
                lines.append("    async fn %s(&self, event: %s) -> anyhow::Result<()>;" % (snake(f.name), ptype))
            lines.append("}")
            lines.append("")
        if info["receive"]:
            lines.append("/// Consumer for channel `%s` (topic: %s) — implement with your broker client."
                         % (channel, info["topic"] or channel))
            lines.append("#[async_trait]")
            lines.append("pub trait %sConsumer {" % pascal(channel))
            for f in info["receive"]:
                ptype = rust_type(f.payload, set()) if f.payload else "String"
                lines.append("    async fn %s(&self, event: %s) -> anyhow::Result<()>;" % (snake(f.name), ptype))
            lines.append("}")
            lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def emit_cargo_toml(crate_name, command):
    return f"""{generated_header("toml", command)}

[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.8"
tokio = {{ version = "1", features = ["full"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
async-trait = "0.1"
anyhow = "1"
chrono = {{ version = "0.4", features = ["serde"] }}
uuid = {{ version = "1", features = ["v4", "serde"] }}
"""


def emit_rust(ir, out_dir, crate_name=None, command="python -m apigen ..."):
    """Generate the full Rust crate into out_dir. Returns written file paths."""
    crate_name = crate_name or snake(ir.api_name)
    written = []
    w = lambda rel, content: written.append(write_file(out_dir, rel, content))

    w("Cargo.toml", emit_cargo_toml(crate_name, command))
    w("src/main.rs", emit_main(ir.tags(), command))
    w("src/api/mod.rs", emit_api_mod(ir.tags(), command))
    for tag in ir.tags():
        w("src/api/%s.rs" % snake(tag), emit_api_module(tag, ir.endpoints_for_tag(tag), command))

    w("src/model/mod.rs", "\n".join(
        [generated_header("rust", command), ""]
        + ["pub mod %s;" % snake(n) for n in sorted(ir.models)]
        + ([""] + ["pub use %s::*;" % snake(n) for n in sorted(ir.models)] if ir.models else [])
    ) + "\n")
    for name in sorted(ir.models):
        w("src/model/%s.rs" % snake(name), emit_model_file(ir.models[name], command))

    w("src/messaging/mod.rs", emit_messaging_mod(ir.flows_by_channel(), command))

    w("README.md", f"""# {ir.api_name} — Rust skeleton (GENERATED)

Generated from OpenAPI/AsyncAPI specs (version {ir.version}). **Do not edit; regenerate.**

- `src/model/` — data model structs/enums (serde, camelCase JSON).
- `src/api/` — axum handlers + router (one module per OpenAPI tag). Handler bodies are `todo!()`.
- `src/messaging/` — producer/consumer traits per AsyncAPI channel, broker-agnostic.

Hand-written code must live outside these files; regeneration replaces this whole tree.
""")
    return written
