//! Emit a Rust service skeleton from the ApiIR.
//!
//! Layout (fully regenerated on each run — spec is the source of truth):
//!     Cargo.toml
//!     src/main.rs            (router wiring)
//!     src/api/mod.rs         (ApiError + module decls)
//!     src/api/<tag>.rs       (axum handlers + router, one per OpenAPI tag)
//!     src/model/mod.rs       (re-exports)
//!     src/model/<name>.rs    (serde structs/enums)
//!     src/messaging/mod.rs   (async traits per AsyncAPI channel, broker-agnostic)
//!
//! The messaging layer is generated as traits so the same interface works with
//! any broker client (lapin, rdkafka, ...); only the impl is hand-written.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::ir::{ApiIR, ChannelFlows, Endpoint, Model, TypeRef};
use crate::naming::{pascal, snake};
use super::common::{generated_header, write_file};

const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
    "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
    "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do",
    "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
];

fn rname(name: &str) -> String {
    let n = snake(name);
    if RUST_KEYWORDS.contains(&n.as_str()) { format!("{n}_") } else { n }
}

/// Map a TypeRef to a Rust type; register needed imports in `imports`.
pub fn rust_type(tref: &TypeRef, imports: &mut BTreeSet<String>) -> String {
    match tref.kind.as_str() {
        "ref" => tref.ref_name.clone().unwrap_or_default(),
        "string" => match tref.format.as_deref() {
            Some("date") => {
                imports.insert("chrono::NaiveDate".into());
                "NaiveDate".into()
            }
            Some("date-time") => {
                imports.insert("chrono::DateTime".into());
                imports.insert("chrono::Utc".into());
                "DateTime<Utc>".into()
            }
            Some("uuid") => {
                imports.insert("uuid::Uuid".into());
                "Uuid".into()
            }
            Some("byte") | Some("binary") => "Vec<u8>".into(),
            _ => "String".into(),
        },
        "integer" => {
            if tref.format.as_deref() == Some("int32") { "i32".into() } else { "i64".into() }
        }
        "number" => {
            if tref.format.as_deref() == Some("float") { "f32".into() } else { "f64".into() }
        }
        "boolean" => "bool".into(),
        "array" => {
            let item = tref.item.as_deref().cloned().unwrap_or_else(TypeRef::any);
            format!("Vec<{}>", rust_type(&item, imports))
        }
        "map" => {
            imports.insert("std::collections::HashMap".into());
            let value = tref.value.as_deref().cloned().unwrap_or_else(TypeRef::any);
            format!("HashMap<String, {}>", rust_type(&value, imports))
        }
        "object" => {
            imports.insert("std::collections::HashMap".into());
            imports.insert("serde_json::Value".into());
            "HashMap<String, Value>".into()
        }
        _ => {
            imports.insert("serde_json::Value".into());
            "Value".into()
        }
    }
}

pub fn emit_model_file(model: &Model, command: &str) -> String {
    let mut imports: BTreeSet<String> =
        ["serde::Deserialize".to_string(), "serde::Serialize".to_string()].into_iter().collect();
    let mut lines: Vec<String> = vec![generated_header("rust", command), String::new()];
    if model.kind == "enum" {
        let mut variants: Vec<String> = vec![];
        for v in &model.enum_values {
            let pv = pascal(v);
            if !variants.contains(&pv) {
                variants.push(pv);
            }
        }
        let mut body: Vec<String> = vec![
            "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]".into(),
            "#[serde(rename_all = \"SCREAMING_SNAKE_CASE\")]".into(),
        ];
        if let Some(d) = &model.description {
            body.insert(0, format!("/// {}", d.replace('\n', " ")));
        }
        body.push(format!("pub enum {} {{", model.name));
        body.push("    ".to_string() + &variants.join(",\n    ") + ",");
        body.push("}".into());
        for imp in &imports {
            lines.push(format!("use {imp};"));
        }
        lines.push(String::new());
        lines.extend(body);
        return lines.join("\n") + "\n";
    }
    let mut fields: Vec<String> = vec![];
    let mut has_refs = false;
    for f in &model.fields {
        let mut rtype = rust_type(&f.r#type, &mut imports);
        let mut t = Some(&f.r#type);
        while let Some(tt) = t {
            if tt.is_ref() {
                has_refs = true;
            }
            t = match (&tt.item, &tt.value) {
                (Some(i), _) => Some(i.as_ref()),
                (None, Some(v)) => Some(v.as_ref()),
                (None, None) => None,
            };
        }
        if !f.required {
            rtype = format!("Option<{rtype}>");
            fields.push("    #[serde(default, skip_serializing_if = \"Option::is_none\")]".into());
        }
        fields.push(format!("    pub {}: {},", rname(&f.name), rtype));
    }
    if has_refs {
        imports.insert("super::*".into());
    }
    let mut body: Vec<String> = vec![
        "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]".into(),
        "#[serde(rename_all = \"camelCase\")]".into(),
    ];
    if let Some(d) = &model.description {
        body.insert(0, format!("/// {}", d.replace('\n', " ")));
    }
    body.push(format!("pub struct {} {{", model.name));
    body.extend(fields);
    body.push("}".into());
    for imp in &imports {
        lines.push(format!("use {imp};"));
    }
    lines.push(String::new());
    lines.extend(body);
    lines.join("\n") + "\n"
}

fn route_path(path: &str) -> &str {
    // axum 0.8 path syntax is already {param}; nothing to convert
    path
}

pub fn emit_api_module(tag: &str, endpoints: &[&Endpoint], command: &str) -> String {
    let _ = tag;
    let mut imports: BTreeSet<String> = [
        "axum::extract::{Path, Query}".to_string(),
        "axum::routing".to_string(),
        "axum::{Json, Router}".to_string(),
        "serde::Deserialize".to_string(),
        "crate::model::*".to_string(),
        "super::ApiError".to_string(),
    ]
    .into_iter()
    .collect();
    let mut handlers: Vec<String> = vec![];
    let mut structs: Vec<String> = vec![];
    let mut routes: std::collections::BTreeMap<String, Vec<(String, String)>> =
        std::collections::BTreeMap::new();
    let mut eps: Vec<&Endpoint> = endpoints.to_vec();
    eps.sort_by(|a, b| (&a.path, &a.method).cmp(&(&b.path, &b.method)));
    for ep in &eps {
        let handler = snake(&ep.operation_id);
        let path_params: Vec<_> = ep.params.iter().filter(|p| p.location == "path").collect();
        let query_params: Vec<_> = ep.params.iter().filter(|p| p.location == "query").collect();
        let mut args: Vec<String> = vec![];
        if !path_params.is_empty() {
            let sname = format!("{}Path", pascal(&ep.operation_id));
            let mut fields: Vec<String> = vec![];
            for p in &path_params {
                let t = rust_type(&p.r#type, &mut imports);
                fields.push(format!(
                    "    #[serde(rename = \"{}\")]\n    pub {}: {},",
                    p.name,
                    rname(&p.name),
                    t
                ));
            }
            structs.push(format!(
                "#[derive(Debug, Deserialize)]\npub struct {} {{\n{}\n}}\n",
                sname,
                fields.join("\n")
            ));
            args.push(format!("Path(path_params): Path<{sname}>"));
        }
        if !query_params.is_empty() {
            let sname = format!("{}Query", pascal(&ep.operation_id));
            let mut fields: Vec<String> = vec![];
            for p in &query_params {
                let mut t = rust_type(&p.r#type, &mut imports);
                if !p.required {
                    t = format!("Option<{t}>");
                }
                fields.push(format!("    pub {}: {},", rname(&p.name), t));
            }
            structs.push(format!(
                "#[derive(Debug, Deserialize)]\npub struct {} {{\n{}\n}}\n",
                sname,
                fields.join("\n")
            ));
            args.push(format!("Query(query_params): Query<{sname}>"));
        }
        if let Some(rb) = &ep.request_body {
            args.push(format!("Json(body): Json<{}>", rust_type(rb, &mut imports)));
        }
        let ret = if ep.response.is_none() {
            imports.insert("axum::http::StatusCode".into());
            "Result<StatusCode, ApiError>".to_string()
        } else {
            format!("Result<Json<{}>, ApiError>", rust_type(ep.response.as_ref().unwrap(), &mut imports))
        };
        let body = format!("    todo!(\"implement {}\")", ep.operation_id);
        let doc = match &ep.summary {
            Some(s) => format!("/// {}", s.replace('\n', " ")),
            None => String::new(),
        };
        handlers.push(format!(
            "{}\npub async fn {}({}) -> {} {{\n{}\n}}",
            doc,
            handler,
            args.join(", "),
            ret,
            body
        ));
        routes
            .entry(ep.path.clone())
            .or_default()
            .push((ep.method.to_lowercase(), handler));
    }
    let method_order = ["get", "post", "put", "patch", "delete", "head", "options"];
    let mut route_lines: Vec<String> = vec![];
    for (path, methods) in &routes {
        let mut methods = methods.clone();
        methods.sort_by_key(|(m, _)| method_order.iter().position(|o| o == m).unwrap_or(99));
        let chain = methods.iter().map(|(m, h)| format!("{m}({h})")).collect::<Vec<_>>().join(".");
        route_lines.push(format!("        .route(\"{}\", routing::{})", route_path(path), chain));
    }
    let mut lines: Vec<String> = vec![generated_header("rust", command), String::new()];
    let mut imps: Vec<&String> = imports.iter().collect();
    imps.sort_by(|a, b| {
        let ka = if a.starts_with("crate::") || a.starts_with("super::") || a.starts_with("self::") {
            0
        } else {
            1
        };
        let kb = if b.starts_with("crate::") || b.starts_with("super::") || b.starts_with("self::") {
            0
        } else {
            1
        };
        ka.cmp(&kb).then(a.cmp(b))
    });
    for imp in imps {
        lines.push(format!("use {imp};"));
    }
    lines.push(String::new());
    let has_structs = !structs.is_empty();
    lines.extend(structs);
    if has_structs {
        lines.push(String::new());
    }
    lines.extend(handlers);
    lines.push(String::new());
    lines.push("pub fn router() -> Router {".into());
    lines.push("    Router::new()".into());
    lines.extend(route_lines);
    lines.push("}".into());
    lines.join("\n") + "\n"
}

pub fn emit_api_mod(tags: &[String], command: &str) -> String {
    let mut lines: Vec<String> = vec![generated_header("rust", command), String::new()];
    for tag in tags {
        lines.push(format!("pub mod {};", snake(tag)));
    }
    if !tags.is_empty() {
        lines.push(String::new());
    }
    lines.extend(vec![
        "use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};".into(),
        "use serde::Serialize;".into(),
        String::new(),
        "#[derive(Debug, Serialize)]".into(),
        "pub struct ErrorBody {".into(),
        "    pub code: String,".into(),
        "    pub message: String,".into(),
        "}".into(),
        String::new(),
        "#[derive(Debug)]".into(),
        "pub enum ApiError {".into(),
        "    NotFound(String),".into(),
        "    NotImplemented,".into(),
        "    Internal(String),".into(),
        "}".into(),
        String::new(),
        "impl IntoResponse for ApiError {".into(),
        "    fn into_response(self) -> Response {".into(),
        "        let (status, code, message) = match self {".into(),
        "            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, \"NOT_FOUND\", m),".into(),
        "            ApiError::NotImplemented => (StatusCode::NOT_IMPLEMENTED, \"NOT_IMPLEMENTED\", \"handler not implemented\".to_string()),".into(),
        "            ApiError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, \"INTERNAL\", m),".into(),
        "        };".into(),
        "        (status, Json(ErrorBody { code: code.to_string(), message })).into_response()".into(),
        "    }".into(),
        "}".into(),
    ]);
    lines.join("\n") + "\n"
}

pub fn emit_main(tags: &[String], command: &str) -> String {
    let mut lines: Vec<String> = vec![
        generated_header("rust", command),
        String::new(),
        "mod api;".into(),
        "mod messaging;".into(),
        "mod model;".into(),
        String::new(),
        "use axum::Router;".into(),
        String::new(),
        "#[tokio::main]".into(),
        "async fn main() {".into(),
    ];
    if !tags.is_empty() {
        lines.push("    let app = Router::new()".into());
        for (i, tag) in tags.iter().enumerate() {
            let suffix = if i == tags.len() - 1 { ";" } else { "" };
            lines.push(format!("        .merge(api::{}::router()){}", snake(tag), suffix));
        }
    } else {
        lines.push("    let app = Router::new();".into());
    }
    lines.extend(vec![
        "    let listener = tokio::net::TcpListener::bind(\"127.0.0.1:8080\")".into(),
        "        .await".into(),
        "        .expect(\"bind 127.0.0.1:8080\");".into(),
        "    axum::serve(listener, app).await.expect(\"serve\");".into(),
        "}".into(),
    ]);
    lines.join("\n") + "\n"
}

pub fn emit_messaging_mod(channels: &std::collections::BTreeMap<String, ChannelFlows>, command: &str) -> String {
    let mut lines: Vec<String> = vec![generated_header("rust", command), String::new()];
    if channels.is_empty() {
        lines.push("// No AsyncAPI flows in the input specs.".into());
        return lines.join("\n") + "\n";
    }
    lines.push("use async_trait::async_trait;".into());
    lines.push("use crate::model::*;".into());
    lines.push(String::new());
    for (channel, info) in channels {
        if !info.send.is_empty() {
            lines.push(format!(
                "/// Producer for channel `{channel}` (topic: {}) — implement with your broker client.",
                info.topic.as_deref().unwrap_or(channel)
            ));
            lines.push("#[async_trait]".into());
            lines.push(format!("pub trait {}Producer {{", pascal(channel)));
            for f in &info.send {
                let ptype = match &f.payload {
                    Some(p) => rust_type(p, &mut BTreeSet::new()),
                    None => "String".to_string(),
                };
                lines.push(format!(
                    "    async fn {}(&self, event: {ptype}) -> anyhow::Result<()>;",
                    snake(&f.name)
                ));
            }
            lines.push("}".into());
            lines.push(String::new());
        }
        if !info.receive.is_empty() {
            lines.push(format!(
                "/// Consumer for channel `{channel}` (topic: {}) — implement with your broker client.",
                info.topic.as_deref().unwrap_or(channel)
            ));
            lines.push("#[async_trait]".into());
            lines.push(format!("pub trait {}Consumer {{", pascal(channel)));
            for f in &info.receive {
                let ptype = match &f.payload {
                    Some(p) => rust_type(p, &mut BTreeSet::new()),
                    None => "String".to_string(),
                };
                lines.push(format!(
                    "    async fn {}(&self, event: {ptype}) -> anyhow::Result<()>;",
                    snake(&f.name)
                ));
            }
            lines.push("}".into());
            lines.push(String::new());
        }
    }
    lines.join("\n").trim_end().to_string() + "\n"
}

pub fn emit_cargo_toml(crate_name: &str, command: &str) -> String {
    format!(
        r#"{header}

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
"#,
        header = generated_header("toml", command)
    )
}

/// Generate the full Rust crate into out_dir. Returns written file paths.
pub fn emit_rust(
    ir: &ApiIR,
    out_dir: &Path,
    crate_name: Option<&str>,
    command: &str,
) -> Result<Vec<PathBuf>> {
    let crate_name = crate_name.map(|s| s.to_string()).unwrap_or_else(|| snake(&ir.api_name));
    let mut written: Vec<PathBuf> = vec![];
    macro_rules! w {
        ($rel:expr, $content:expr) => {
            written.push(write_file(out_dir, $rel, &$content)?)
        };
    }

    w!("Cargo.toml", emit_cargo_toml(&crate_name, command));
    w!("src/main.rs", emit_main(&ir.tags(), command));
    w!("src/api/mod.rs", emit_api_mod(&ir.tags(), command));
    for tag in ir.tags() {
        w!(
            &format!("src/api/{}.rs", snake(&tag)),
            emit_api_module(&tag, &ir.endpoints_for_tag(&tag), command)
        );
    }

    let mut mlines: Vec<String> = vec![generated_header("rust", command), String::new()];
    for n in ir.models.keys() {
        mlines.push(format!("pub mod {};", snake(n)));
    }
    if !ir.models.is_empty() {
        mlines.push(String::new());
        for n in ir.models.keys() {
            mlines.push(format!("pub use {}::*;", snake(n)));
        }
    }
    w!("src/model/mod.rs", mlines.join("\n") + "\n");
    for (name, model) in &ir.models {
        w!(&format!("src/model/{}.rs", snake(name)), emit_model_file(model, command));
    }

    w!("src/messaging/mod.rs", emit_messaging_mod(&ir.flows_by_channel(), command));

    w!(
        "README.md",
        format!(
            r#"# {api_name} — Rust skeleton (GENERATED)

Generated from OpenAPI/AsyncAPI specs (version {version}). **Do not edit; regenerate.**

- `src/model/` — data model structs/enums (serde, camelCase JSON).
- `src/api/` — axum handlers + router (one module per OpenAPI tag). Handler bodies are `todo!()`.
- `src/messaging/` — producer/consumer traits per AsyncAPI channel, broker-agnostic.

Hand-written code must live outside these files; regeneration replaces this whole tree.
"#,
            api_name = ir.api_name,
            version = ir.version
        )
    );
    Ok(written)
}
