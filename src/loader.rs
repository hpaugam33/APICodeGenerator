//! Load OpenAPI 3.x and AsyncAPI 2.x/3.x documents into the ApiIR.
//!
//! Supports local pointers (#/...) and external file refs (./other.yaml#/...),
//! which is how the AsyncAPI file references the shared data model defined in
//! the OpenAPI file.

use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use yaml_rust2::yaml::Hash;
use yaml_rust2::{Yaml, YamlLoader};

use crate::ir::{ApiIR, Endpoint, Field, Flow, Model, Param, TypeRef};
use crate::naming::{pascal, snake};

pub const HTTP_METHODS: [&str; 8] =
    ["get", "put", "post", "delete", "patch", "head", "options", "trace"];

// ---------------------------------------------------------------------------
// small YAML helpers (Python dict/list/scalar semantics)
// ---------------------------------------------------------------------------

pub fn as_hash(y: &Yaml) -> Option<&Hash> {
    match y {
        Yaml::Hash(h) => Some(h),
        _ => None,
    }
}

pub fn as_array(y: &Yaml) -> Option<&Vec<Yaml>> {
    match y {
        Yaml::Array(a) => Some(a),
        _ => None,
    }
}

pub fn get<'a>(y: &'a Yaml, key: &str) -> Option<&'a Yaml> {
    as_hash(y).and_then(|h| h.get(&Yaml::String(key.to_string())))
}

fn ystr(s: &str) -> Yaml {
    Yaml::String(s.to_string())
}

/// Python `str()` equivalent for scalar YAML values.
pub fn scalar_str(y: &Yaml) -> String {
    match y {
        Yaml::String(s) => s.clone(),
        Yaml::Integer(i) => i.to_string(),
        Yaml::Real(s) => s.clone(),
        Yaml::Boolean(b) => if *b { "True".to_string() } else { "False".to_string() },
        Yaml::Null => "None".to_string(),
        other => format!("{other:?}"),
    }
}

/// Python `bool()` equivalent.
pub fn truthy(y: &Yaml) -> bool {
    match y {
        Yaml::Null => false,
        Yaml::Boolean(b) => *b,
        Yaml::String(s) => !s.is_empty(),
        Yaml::Integer(i) => *i != 0,
        Yaml::Real(s) => s.parse::<f64>().map(|f| f != 0.0).unwrap_or(true),
        Yaml::Array(a) => !a.is_empty(),
        Yaml::Hash(h) => !h.is_empty(),
        Yaml::Alias(_) | Yaml::BadValue => true,
    }
}

/// `.get(key)` that treats an explicit null like a missing key (Python `or` chains).
fn get_or<'a>(y: &'a Yaml, key: &str, fallback: &'a Yaml) -> &'a Yaml {
    match get(y, key) {
        Some(v) if !matches!(v, Yaml::Null) => v,
        _ => fallback,
    }
}

fn or_default(y: &Yaml, key: &str) -> Yaml {
    match get(y, key) {
        Some(v) if truthy(v) => v.clone(),
        _ => Yaml::Null,
    }
}

/// Lexical absolute path (like Python os.path.abspath).
pub fn abspath(p: &Path) -> PathBuf {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(p)
    };
    normalize_path(&abs)
}

fn normalize_path(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// $ref resolution
// ---------------------------------------------------------------------------

/// Caches parsed documents and resolves $ref pointers across files.
pub struct SpecContext {
    docs: HashMap<PathBuf, Yaml>,
}

impl SpecContext {
    pub fn new() -> Self {
        SpecContext { docs: HashMap::new() }
    }

    pub fn load_doc(&mut self, path: &Path) -> Result<Yaml> {
        let ap = abspath(path);
        if let Some(d) = self.docs.get(&ap) {
            return Ok(d.clone());
        }
        let text = std::fs::read_to_string(&ap)
            .with_context(|| format!("cannot read {}", ap.display()))?;
        let docs = YamlLoader::load_from_str(&text)
            .with_context(|| format!("invalid YAML in {}", ap.display()))?;
        let doc = docs.into_iter().next().unwrap_or(Yaml::Null);
        self.docs.insert(ap, doc.clone());
        Ok(doc)
    }

    /// Return (target_object, file the target lives in).
    pub fn resolve_ref(&mut self, r: &str, current_file: &Path) -> Result<(Yaml, PathBuf)> {
        if let Some(ptr) = r.strip_prefix('#') {
            let doc = self.load_doc(current_file)?;
            return Ok((pointer(&doc, ptr)?, abspath(current_file)));
        }
        let (file_part, ptr) = match r.split_once('#') {
            Some((f, p)) => (f, Some(p)),
            None => (r, None),
        };
        let base = abspath(current_file);
        let dir = base.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        let target_file = normalize_path(&dir.join(file_part));
        let doc = self.load_doc(&target_file)?;
        if let Some(ptr) = ptr {
            return Ok((pointer(&doc, &format!("/{ptr}"))?, target_file));
        }
        Ok((doc, target_file))
    }
}

impl Default for SpecContext {
    fn default() -> Self {
        Self::new()
    }
}

pub fn pointer(doc: &Yaml, ptr: &str) -> Result<Yaml> {
    let mut cur = doc.clone();
    for seg in ptr.split('/') {
        if seg.is_empty() {
            continue;
        }
        let seg = seg.replace("~1", "/").replace("~0", "~");
        let h = as_hash(&cur)
            .ok_or_else(|| anyhow!("$ref segment '{seg}' on a non-mapping value"))?;
        cur = h
            .get(&ystr(&seg))
            .cloned()
            .ok_or_else(|| anyhow!("$ref segment '{seg}' not found"))?;
    }
    Ok(cur)
}

pub fn ref_name(r: &str) -> String {
    r.split('/').next_back().unwrap_or("").to_string()
}

// ---------------------------------------------------------------------------
// schema -> TypeRef
// ---------------------------------------------------------------------------

pub fn schema_to_ref(ctx: &mut SpecContext, file: &Path, schema: &Yaml) -> Result<TypeRef> {
    let Some(_h) = as_hash(schema) else {
        return Ok(TypeRef::any());
    };
    if let Some(r) = get(schema, "$ref") {
        return Ok(TypeRef::ref_(&ref_name(&scalar_str(r))));
    }
    if let Some(allof) = get(schema, "allOf") {
        if let Some(arr) = as_array(allof) {
            if arr.len() == 1 && as_hash(&arr[0]).map(|h| h.contains_key(&ystr("$ref"))).unwrap_or(false) {
                return schema_to_ref(ctx, file, &arr[0]);
            }
        }
    }
    if get(schema, "enum").is_some() {
        // inline enum used directly as a property: caller registers a model for it
        return Ok(TypeRef::any());
    }
    let mut t: Option<String> = get(schema, "type").map(scalar_str);
    if let Some(arr) = get(schema, "type").and_then(as_array) {
        t = arr.iter().map(scalar_str).find(|x| x != "null");
    }
    if t.as_deref() == Some("array") {
        let items = get(schema, "items").cloned().unwrap_or(Yaml::Null);
        return Ok(TypeRef::array(schema_to_ref(ctx, file, &items)?));
    }
    if t.as_deref() == Some("object")
        || get(schema, "properties").is_some()
        || get(schema, "additionalProperties").is_some()
    {
        let ap = get(schema, "additionalProperties");
        if ap.and_then(as_hash).is_some() {
            return Ok(TypeRef::map(schema_to_ref(ctx, file, ap.unwrap())?));
        }
        let ap_true = matches!(ap, Some(Yaml::Boolean(true)));
        let no_props = get(schema, "properties").is_none();
        if ap_true || (t.as_deref() == Some("object") && no_props) {
            return Ok(TypeRef::map(TypeRef::any()));
        }
        return Ok(TypeRef::new("object")); // inline object; build_model registers a named model
    }
    match t.as_deref() {
        Some("integer") => Ok(TypeRef::with_format("integer", get(schema, "format").map(scalar_str))),
        Some("number") => Ok(TypeRef::with_format("number", get(schema, "format").map(scalar_str))),
        Some("boolean") => Ok(TypeRef::new("boolean")),
        Some("string") => Ok(TypeRef::with_format("string", get(schema, "format").map(scalar_str))),
        _ => Ok(TypeRef::any()),
    }
}

// ---------------------------------------------------------------------------
// models
// ---------------------------------------------------------------------------

pub const CONSTRAINT_KEYS: [&str; 12] = [
    "minLength",
    "maxLength",
    "pattern",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "minItems",
    "maxItems",
    "multipleOf",
    "default",
    "format",
];

/// Return a TypeRef for a property, registering inline object/enum models.
pub fn property_ref(
    ctx: &mut SpecContext,
    file: &Path,
    models: &mut BTreeMap<String, Model>,
    parent_name: &str,
    prop_name: &str,
    schema: &Yaml,
) -> Result<TypeRef> {
    if as_hash(schema).is_some() {
        if let Some(r) = get(schema, "$ref") {
            let name = ref_name(&scalar_str(r));
            if !models.contains_key(&name) {
                let (target, target_file) = ctx.resolve_ref(&scalar_str(r), file)?;
                build_model(ctx, &target_file, models, &name, &target)?;
            }
            return Ok(TypeRef::ref_(&name));
        }
        if get(schema, "allOf").is_some() && !truthy(get(schema, "properties").unwrap_or(&Yaml::Null)) {
            let mut merged = Hash::new();
            for part in as_array(get(schema, "allOf").unwrap()).cloned().unwrap_or_default() {
                let mut resolved = part.clone();
                if let Some(ph) = as_hash(&part) {
                    if let Some(pr) = ph.get(&ystr("$ref")) {
                        let (t, _) = ctx.resolve_ref(&scalar_str(pr), file)?;
                        resolved = t;
                    }
                }
                if let Some(rh) = as_hash(&resolved) {
                    for (k, v) in rh {
                        merged.insert(k.clone(), v.clone());
                    }
                }
            }
            return property_ref(ctx, file, models, parent_name, prop_name, &Yaml::Hash(merged));
        }
        if get(schema, "enum").is_some() {
            let mname = format!("{}{}", parent_name, pascal(prop_name));
            if !models.contains_key(&mname) {
                let values = as_array(get(schema, "enum").unwrap())
                    .map(|a| a.iter().map(scalar_str).collect())
                    .unwrap_or_default();
                models.insert(
                    mname.clone(),
                    Model {
                        name: mname.clone(),
                        kind: "enum".to_string(),
                        description: get(schema, "description").map(scalar_str),
                        fields: vec![],
                        enum_values: values,
                    },
                );
            }
            return Ok(TypeRef::ref_(&mname));
        }
        if scalar_str(get_or(schema, "type", &Yaml::Null)) == "object"
            && truthy(get(schema, "properties").unwrap_or(&Yaml::Null))
        {
            let mname = format!("{}{}", parent_name, pascal(prop_name));
            if !models.contains_key(&mname) {
                build_model(ctx, file, models, &mname, schema)?;
            }
            return Ok(TypeRef::ref_(&mname));
        }
    }
    schema_to_ref(ctx, file, schema)
}

/// Recursively register a model for every $ref found in a schema tree.
pub fn register_refs(ctx: &mut SpecContext, file: &Path, models: &mut BTreeMap<String, Model>, schema: &Yaml) -> Result<()> {
    if let Some(h) = as_hash(schema) {
        if let Some(r) = h.get(&ystr("$ref")) {
            let name = ref_name(&scalar_str(r));
            if !models.contains_key(&name) {
                let (target, tfile) = ctx.resolve_ref(&scalar_str(r), file)?;
                build_model(ctx, &tfile, models, &name, &target)?;
            }
        } else {
            for v in h.values() {
                register_refs(ctx, file, models, v)?;
            }
        }
    } else if let Some(arr) = as_array(schema) {
        for v in arr {
            register_refs(ctx, file, models, v)?;
        }
    }
    Ok(())
}

pub fn build_model(
    ctx: &mut SpecContext,
    file: &Path,
    models: &mut BTreeMap<String, Model>,
    name: &str,
    schema: &Yaml,
) -> Result<()> {
    if models.contains_key(name) {
        return Ok(());
    }
    if as_hash(schema).is_none() {
        return Ok(());
    }
    if let Some(r) = get(schema, "$ref") {
        let (target, target_file) = ctx.resolve_ref(&scalar_str(r), file)?;
        let rname = ref_name(&scalar_str(r));
        return build_model(ctx, &target_file, models, &rname, &target);
    }
    let schema_owned;
    let schema = if get(schema, "allOf").is_some() {
        let mut props = Hash::new();
        let mut required: Vec<String> = vec![];
        let mut desc: Option<String> = get(schema, "description").map(scalar_str);
        for part in as_array(get(schema, "allOf").unwrap()).cloned().unwrap_or_default() {
            let mut resolved = part.clone();
            if let Some(ph) = as_hash(&part) {
                if let Some(pr) = ph.get(&ystr("$ref")) {
                    let (t, _) = ctx.resolve_ref(&scalar_str(pr), file)?;
                    let rname = ref_name(&scalar_str(pr));
                    if !models.contains_key(&rname) {
                        build_model(ctx, file, models, &rname, &t)?;
                    }
                    resolved = t;
                }
            }
            if let Some(rh) = as_hash(&resolved) {
                if let Some(rp) = rh.get(&ystr("properties")).and_then(as_hash) {
                    for (k, v) in rp {
                        props.insert(k.clone(), v.clone());
                    }
                }
                if let Some(rreq) = rh.get(&ystr("required")).and_then(as_array) {
                    for r in rreq {
                        let rs = scalar_str(r);
                        if !required.contains(&rs) {
                            required.push(rs);
                        }
                    }
                }
                if desc.is_none() {
                    desc = rh.get(&ystr("description")).map(scalar_str);
                }
            }
        }
        let mut h = Hash::new();
        h.insert(ystr("type"), ystr("object"));
        h.insert(ystr("properties"), Yaml::Hash(props));
        h.insert(
            ystr("required"),
            Yaml::Array(required.iter().map(|s| ystr(s)).collect()),
        );
        if let Some(d) = desc {
            h.insert(ystr("description"), ystr(&d));
        }
        let merged = Yaml::Hash(h);
        schema_owned = merged.clone();
        &schema_owned
    } else {
        schema
    };
    if let Some(arr) = get(schema, "enum").and_then(as_array) {
        models.insert(
            name.to_string(),
            Model {
                name: name.to_string(),
                kind: "enum".to_string(),
                description: get(schema, "description").map(scalar_str),
                fields: vec![],
                enum_values: arr.iter().map(scalar_str).collect(),
            },
        );
        return Ok(());
    }
    let props = get(schema, "properties").and_then(as_hash).cloned().unwrap_or_default();
    let required: std::collections::HashSet<String> = get(schema, "required")
        .and_then(as_array)
        .map(|a| a.iter().map(scalar_str).collect())
        .unwrap_or_default();
    let mut fields = vec![];
    for (pname_y, pschema) in &props {
        let pname = scalar_str(pname_y);
        let pschema = if as_hash(pschema).is_some() { pschema } else { &Yaml::Null };
        let tref = property_ref(ctx, file, models, name, &pname, pschema)?;
        let mut constraints = BTreeMap::new();
        for k in CONSTRAINT_KEYS {
            if let Some(v) = get(pschema, k) {
                constraints.insert(k.to_string(), v.clone());
            }
        }
        fields.push(Field {
            name: pname.clone(),
            r#type: tref,
            required: required.contains(&pname),
            description: get(pschema, "description").map(scalar_str),
            constraints,
        });
    }
    models.insert(
        name.to_string(),
        Model {
            name: name.to_string(),
            kind: "object".to_string(),
            description: get(schema, "description").map(scalar_str),
            fields,
            enum_values: vec![],
        },
    );
    Ok(())
}

fn load_schemas(
    ctx: &mut SpecContext,
    file: &Path,
    doc: &Yaml,
    models: &mut BTreeMap<String, Model>,
) -> Result<()> {
    let comps = get(doc, "components")
        .and_then(|c| get(c, "schemas"))
        .and_then(as_hash)
        .cloned()
        .unwrap_or_default();
    for (name_y, schema) in &comps {
        build_model(ctx, file, models, &scalar_str(name_y), schema)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// OpenAPI
// ---------------------------------------------------------------------------

fn content_schema(content: &Yaml) -> Option<Yaml> {
    let h = as_hash(content)?;
    for mt in ["application/json", "application/*+json", "*/*"] {
        if let Some(entry) = h.get(&ystr(mt)) {
            return if truthy(entry) { get(entry, "schema").cloned() } else { None };
        }
    }
    for (mt, entry) in h {
        if scalar_str(mt).contains("json") {
            return if truthy(entry) { get(entry, "schema").cloned() } else { None };
        }
    }
    None
}

pub fn load_openapi(
    ctx: &mut SpecContext,
    path: &Path,
    models: &mut BTreeMap<String, Model>,
) -> Result<(Yaml, Vec<Endpoint>)> {
    let doc = ctx.load_doc(path)?;
    load_schemas(ctx, path, &doc, models)?;
    let mut endpoints = vec![];
    let paths = get(&doc, "paths").and_then(as_hash).cloned().unwrap_or_default();
    for (path_y, item) in &paths {
        let path_str = scalar_str(path_y);
        let Some(item_h) = as_hash(item) else { continue };
        let shared_params: Vec<Yaml> = get(item, "parameters")
            .and_then(as_array)
            .map(|a| a.iter().filter(|p| as_hash(p).is_some()).cloned().collect())
            .unwrap_or_default();
        for (method_y, op) in item_h {
            let method = scalar_str(method_y);
            if !HTTP_METHODS.contains(&method.as_str()) || as_hash(op).is_none() {
                continue;
            }
            let op_id = match get(op, "operationId") {
                Some(v) if truthy(v) => scalar_str(v),
                _ => snake(&format!("{method} {path_str}")),
            };
            // merged params keyed by (name, in); op-level overrides shared, keeping position
            let mut merged: Vec<(String, String, Yaml)> = vec![];
            let mut upsert = |key: (String, String), p: Yaml| {
                if let Some(e) =
                    merged.iter_mut().find(|(n, i, _)| (n.as_str(), i.as_str()) == (key.0.as_str(), key.1.as_str()))
                {
                    e.2 = p;
                } else {
                    merged.push((key.0, key.1, p));
                }
            };
            for p in &shared_params {
                let key = (
                    scalar_str(get_or(p, "name", &Yaml::Null)),
                    scalar_str(get_or(p, "in", &Yaml::Null)),
                );
                upsert(key, p.clone());
            }
            for p in get(op, "parameters").and_then(as_array).cloned().unwrap_or_default() {
                if as_hash(&p).is_none() {
                    continue;
                }
                let key = (
                    scalar_str(get_or(&p, "name", &Yaml::Null)),
                    scalar_str(get_or(&p, "in", &Yaml::Null)),
                );
                upsert(key, p);
            }
            merged.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
            let mut params = vec![];
            for (pname, pin, p) in &merged {
                let pschema = get(p, "schema").cloned().unwrap_or(Yaml::Null);
                register_refs(ctx, path, models, &pschema)?;
                params.push(Param {
                    name: pname.clone(),
                    location: pin.clone(),
                    r#type: schema_to_ref(ctx, path, &pschema)?,
                    required: truthy(get(p, "required").unwrap_or(&Yaml::Null)) || pin == "path",
                    description: get(p, "description").map(scalar_str),
                });
            }
            let mut body: Option<TypeRef> = None;
            let rb = or_default(op, "requestBody");
            if let Some(bs) = content_schema(get(&rb, "content").unwrap_or(&Yaml::Null)) {
                register_refs(ctx, path, models, &bs)?;
                body = Some(schema_to_ref(ctx, path, &bs)?);
            }
            let responses = get(op, "responses").and_then(as_hash).cloned().unwrap_or_default();
            let mut codes: Vec<(String, Yaml)> =
                responses.into_iter().map(|(k, v)| (scalar_str(&k), v)).collect();
            codes.sort_by(|a, b| a.0.cmp(&b.0));
            let mut status = "200".to_string();
            let mut rschema: Option<Yaml> = None;
            let mut eschema: Option<Yaml> = None;
            for (code, rv) in &codes {
                let rv2 = if truthy(rv) { rv.clone() } else { Yaml::Null };
                let content = get(&rv2, "content").cloned().unwrap_or(Yaml::Null);
                let s = match content_schema(&content) {
                    Some(s) => {
                        register_refs(ctx, path, models, &s)?;
                        Some(s)
                    }
                    None => None,
                };
                if code.starts_with('2') && rschema.is_none() {
                    status = code.clone();
                    rschema = s;
                } else if code.starts_with('4') || code.starts_with('5') {
                    if eschema.is_none() {
                        eschema = s;
                    }
                }
            }
            let resp = match &rschema {
                Some(s) if truthy(s) => Some(schema_to_ref(ctx, path, s)?),
                _ => None,
            };
            let err = match &eschema {
                Some(s) if truthy(s) => Some(schema_to_ref(ctx, path, s)?),
                _ => None,
            };
            let tag = match get(op, "tags").and_then(as_array).and_then(|t| t.first()) {
                Some(t) if truthy(t) => scalar_str(t),
                _ => "default".to_string(),
            };
            endpoints.push(Endpoint {
                method: method.to_uppercase(),
                path: path_str.clone(),
                operation_id: op_id,
                tag,
                summary: get(op, "summary")
                    .filter(|s| truthy(s))
                    .map(scalar_str)
                    .or_else(|| get(op, "description").filter(|s| truthy(s)).map(scalar_str)),
                params,
                request_body: body,
                response: resp,
                response_status: status,
                error_response: err,
            });
        }
    }
    Ok((doc, endpoints))
}

// ---------------------------------------------------------------------------
// AsyncAPI
// ---------------------------------------------------------------------------

fn channel_topic(channel_def: &Yaml, address: &str) -> String {
    let bindings = or_default(channel_def, "bindings");
    let kafka = or_default(&bindings, "kafka");
    match get(&kafka, "topic") {
        Some(t) if truthy(t) => scalar_str(t),
        _ => address.to_string(),
    }
}

fn message_payload(
    ctx: &mut SpecContext,
    file: &Path,
    models: &mut BTreeMap<String, Model>,
    msg_ref: Option<&str>,
    inline_msg: &Yaml,
) -> Result<(Option<TypeRef>, Option<String>)> {
    let mut msg = if truthy(inline_msg) { inline_msg.clone() } else { Yaml::Hash(Hash::new()) };
    let mut file = file.to_path_buf();
    if let Some(mr) = msg_ref {
        let (m, f) = ctx.resolve_ref(mr, &file)?;
        msg = m;
        file = f;
    }
    let mut seen: Vec<String> = vec![];
    loop {
        let Some(h) = as_hash(&msg) else { break };
        let has_ref = h.contains_key(&ystr("$ref"));
        let has_payload = h.contains_key(&ystr("payload"));
        if !(has_ref && !has_payload) {
            break;
        }
        let r = scalar_str(h.get(&ystr("$ref")).unwrap());
        if seen.contains(&r) {
            break;
        }
        seen.push(r.clone());
        let (m, f) = ctx.resolve_ref(&r, &file)?;
        msg = m;
        file = f;
    }
    let payload = match get(&msg, "payload") {
        Some(p) if !matches!(p, Yaml::Null) => Some(p.clone()),
        _ => None,
    };
    let Some(payload) = payload else {
        return Ok((None, get(&msg, "name").map(scalar_str)));
    };
    if let Some(ph) = as_hash(&payload) {
        if let Some(pr) = ph.get(&ystr("$ref")) {
            let name = ref_name(&scalar_str(pr));
            if !models.contains_key(&name) {
                let (target, tfile) = ctx.resolve_ref(&scalar_str(pr), &file)?;
                build_model(ctx, &tfile, models, &name, &target)?;
            }
            let mn = match get(&msg, "name") {
                Some(n) if truthy(n) => scalar_str(n),
                _ => name.clone(),
            };
            return Ok((Some(TypeRef::ref_(&name)), Some(mn)));
        }
    }
    let name = match get(&msg, "name") {
        Some(n) if truthy(n) => scalar_str(n),
        _ => "Message".to_string(),
    };
    let mname = format!("{}Payload", pascal(&name));
    if !models.contains_key(&mname) {
        build_model(ctx, &file, models, &mname, &payload)?;
    }
    let mn = match get(&msg, "name") {
        Some(n) if truthy(n) => scalar_str(n),
        _ => name,
    };
    Ok((Some(TypeRef::ref_(&mname)), Some(mn)))
}

pub fn load_asyncapi(
    ctx: &mut SpecContext,
    path: &Path,
    models: &mut BTreeMap<String, Model>,
) -> Result<(Yaml, Vec<Flow>)> {
    let doc = ctx.load_doc(path)?;
    load_schemas(ctx, path, &doc, models)?;
    let version = match get(&doc, "asyncapi") {
        Some(v) => scalar_str(v),
        None => "3.0.0".to_string(),
    };
    let mut flows = vec![];
    if version.starts_with('3') {
        let channels = get(&doc, "channels").and_then(as_hash).cloned().unwrap_or_default();
        let mut chan_info: HashMap<String, (String, String)> = HashMap::new();
        for (cname_y, cdef) in &channels {
            let cname = scalar_str(cname_y);
            let address = match get(cdef, "address") {
                Some(a) if truthy(a) => scalar_str(a),
                _ => cname.clone(),
            };
            let topic = channel_topic(cdef, &address);
            chan_info.insert(cname, (address, topic));
        }
        let operations = get(&doc, "operations").and_then(as_hash).cloned().unwrap_or_default();
        for (op_id_y, op) in &operations {
            let op_id = scalar_str(op_id_y);
            let chan_ref = get(op, "channel")
                .and_then(|c| get(c, "$ref"))
                .map(scalar_str)
                .unwrap_or_default();
            let cname = ref_name(&chan_ref);
            let (address, topic) = chan_info
                .get(&cname)
                .cloned()
                .unwrap_or_else(|| (cname.clone(), cname.clone()));
            let msgs = get(op, "messages").and_then(as_array).cloned().unwrap_or_default();
            let mut payload: Option<TypeRef> = None;
            let mut mname: Option<String> = None;
            if !msgs.is_empty() {
                let m0 = &msgs[0];
                let mr = get(m0, "$ref").map(scalar_str);
                let (p, m) = message_payload(ctx, path, models, mr.as_deref(), m0)?;
                payload = p;
                mname = m;
            } else if truthy(get(op, "channel").unwrap_or(&Yaml::Null)) {
                // no messages on the operation: fall through to channel messages below
            }
            if payload.is_none() {
                if let Some(chan) = channels.get(&ystr(&cname)) {
                    let cmessages = get(chan, "messages").and_then(as_hash).cloned().unwrap_or_default();
                    if let Some((_, first)) = cmessages.iter().next() {
                        let (p, m) = message_payload(ctx, path, models, None, first)?;
                        payload = p;
                        mname = m;
                    }
                }
            }
            let action = match get(op, "action") {
                Some(a) if truthy(a) => scalar_str(a),
                _ => "receive".to_string(),
            };
            let description = get(op, "description")
                .filter(|d| truthy(d))
                .map(scalar_str)
                .or_else(|| get(op, "summary").filter(|s| truthy(s)).map(scalar_str));
            flows.push(Flow {
                name: op_id,
                action,
                channel: address,
                topic: Some(topic),
                message_name: mname,
                payload,
                description,
            });
        }
    } else {
        // AsyncAPI 2.x
        let channels = get(&doc, "channels").and_then(as_hash).cloned().unwrap_or_default();
        for (cname_y, cdef) in &channels {
            let cname = scalar_str(cname_y);
            for (action, op_key) in [("send", "publish"), ("receive", "subscribe")] {
                let Some(op) = get(cdef, op_key).filter(|o| truthy(o)) else { continue };
                let msgs = get(op, "messages").and_then(as_array).cloned().unwrap_or_default();
                let inline = get(op, "message").unwrap_or(&Yaml::Null);
                let (payload, mname) = if !msgs.is_empty() {
                    let mr = get(&msgs[0], "$ref").map(scalar_str);
                    message_payload(ctx, path, models, mr.as_deref(), &msgs[0])?
                } else {
                    message_payload(ctx, path, models, None, inline)?
                };
                let op_id = match get(op, "operationId") {
                    Some(v) if truthy(v) => scalar_str(v),
                    _ => snake(&format!("{action} {cname} {}", mname.clone().unwrap_or_default())),
                };
                let description = get(op, "summary")
                    .filter(|s| truthy(s))
                    .map(scalar_str)
                    .or_else(|| get(op, "description").filter(|d| truthy(d)).map(scalar_str));
                flows.push(Flow {
                    name: op_id,
                    action: action.to_string(),
                    channel: cname.clone(),
                    topic: Some(cname.clone()),
                    message_name: mname,
                    payload,
                    description,
                });
            }
        }
    }
    Ok((doc, flows))
}

// ---------------------------------------------------------------------------
// entry point
// ---------------------------------------------------------------------------

pub fn load_all(
    openapi_files: &[PathBuf],
    asyncapi_files: &[PathBuf],
    api_name: Option<&str>,
) -> Result<ApiIR> {
    let mut ctx = SpecContext::new();
    let mut models: BTreeMap<String, Model> = BTreeMap::new();
    let mut endpoints = vec![];
    let mut flows = vec![];
    let mut sources = vec![];
    let mut first_doc: Option<Yaml> = None;
    for path in openapi_files {
        let (doc, eps) = load_openapi(&mut ctx, path, &mut models)?;
        if first_doc.is_none() {
            first_doc = Some(doc);
        }
        endpoints.extend(eps);
        sources.push(path.display().to_string());
    }
    for path in asyncapi_files {
        let (doc, fls) = load_asyncapi(&mut ctx, path, &mut models)?;
        if first_doc.is_none() {
            first_doc = Some(doc);
        }
        flows.extend(fls);
        sources.push(path.display().to_string());
    }
    let info = first_doc.as_ref().and_then(|d| get(d, "info")).cloned().unwrap_or(Yaml::Null);
    let name = api_name
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| get(&info, "title").filter(|t| truthy(t)).map(scalar_str))
        .unwrap_or_else(|| "Generated API".to_string());
    let version = match get(&info, "version") {
        Some(v) if truthy(v) => scalar_str(v),
        _ => "1.0.0".to_string(),
    };
    endpoints.sort_by(|a, b| (&a.tag, &a.path, &a.method).cmp(&(&b.tag, &b.path, &b.method)));
    flows.sort_by(|a, b| (&a.channel, &a.action, &a.name).cmp(&(&b.channel, &b.action, &b.name)));
    Ok(ApiIR { api_name: name, version, models, endpoints, flows, sources })
}
