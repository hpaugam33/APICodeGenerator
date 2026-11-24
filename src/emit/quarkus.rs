//! Emit a Java Quarkus service skeleton from the ApiIR.
//!
//! Layout (fully regenerated on each run — spec is the source of truth):
//!     pom.xml
//!     src/main/resources/application.properties   (messaging channels config)
//!     src/main/java/<pkg>/model/*.java             (records + enums, Bean Validation)
//!     src/main/java/<pkg>/api/*Resource.java       (JAX-RS resources, one per OpenAPI tag)
//!     src/main/java/<pkg>/messaging/*.java         (producers/consumers per AsyncAPI channel)

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::ir::{ApiIR, Endpoint, Field, Flow, Model, Param, TypeRef};
use crate::loader::{scalar_str, truthy};
use crate::naming::{camel, pascal};
use super::common::{common_path_prefix, generated_header, write_file};

const JAVA_KEYWORDS: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class",
    "const", "continue", "default", "do", "double", "else", "enum", "extends", "final",
    "finally", "float", "for", "goto", "if", "implements", "import", "instanceof", "int",
    "interface", "long", "native", "new", "package", "private", "protected", "public",
    "return", "short", "static", "strictfp", "super", "switch", "synchronized", "this",
    "throw", "throws", "transient", "try", "void", "volatile", "while", "true", "false", "null",
];

const QUARKUS_VERSION: &str = "3.15.1";

fn jname(name: &str) -> String {
    let n = camel(name);
    if JAVA_KEYWORDS.contains(&n.as_str()) { format!("{n}_") } else { n }
}

fn collect_refs(tref: Option<&TypeRef>, out: &mut BTreeSet<String>) {
    let Some(t) = tref else { return };
    if t.is_ref() {
        out.insert(t.ref_name.clone().unwrap_or_default());
    }
    collect_refs(t.item.as_deref(), out);
    collect_refs(t.value.as_deref(), out);
}

fn model_imports(package: &str, trefs: &[Option<&TypeRef>]) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    for t in trefs {
        collect_refs(*t, &mut refs);
    }
    refs.into_iter().map(|r| format!("{package}.model.{r}")).collect()
}

/// Map a TypeRef to a Java type; register needed imports in `imports`.
pub fn java_type(tref: &TypeRef, imports: &mut BTreeSet<String>) -> String {
    match tref.kind.as_str() {
        "ref" => tref.ref_name.clone().unwrap_or_default(),
        "string" => match tref.format.as_deref() {
            Some("date") => {
                imports.insert("java.time.LocalDate".into());
                "LocalDate".into()
            }
            Some("date-time") => {
                imports.insert("java.time.OffsetDateTime".into());
                "OffsetDateTime".into()
            }
            Some("uuid") => {
                imports.insert("java.util.UUID".into());
                "UUID".into()
            }
            Some("byte") | Some("binary") => "byte[]".into(),
            _ => "String".into(),
        },
        "integer" => {
            if tref.format.as_deref() == Some("int32") { "Integer".into() } else { "Long".into() }
        }
        "number" => {
            if tref.format.as_deref() == Some("float") { "Float".into() } else { "Double".into() }
        }
        "boolean" => "Boolean".into(),
        "array" => {
            imports.insert("java.util.List".into());
            let item = tref.item.as_deref().cloned().unwrap_or_else(TypeRef::any);
            format!("List<{}>", java_type(&item, imports))
        }
        "map" => {
            imports.insert("java.util.Map".into());
            let value = tref.value.as_deref().cloned().unwrap_or_else(TypeRef::any);
            format!("Map<String, {}>", java_type(&value, imports))
        }
        "object" => {
            imports.insert("java.util.Map".into());
            "Map<String, Object>".into()
        }
        _ => "Object".into(),
    }
}

fn java_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn capitalize(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &ch.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn field_annotations(field: &Field, imports: &mut BTreeSet<String>) -> Vec<String> {
    let mut anns: Vec<String> = vec![];
    if field.required {
        anns.push("@NotNull".into());
        imports.insert("jakarta.validation.constraints.NotNull".into());
    }
    let c = &field.constraints;
    let t = &field.r#type;
    match t.kind.as_str() {
        "string" => {
            if c.get("format").map(scalar_str).as_deref() == Some("email") {
                anns.push("@Email".into());
                imports.insert("jakarta.validation.constraints.Email".into());
            }
            let mut size: Vec<String> = vec![];
            if let Some(v) = c.get("minLength") {
                size.push(format!("min = {}", scalar_str(v)));
            }
            if let Some(v) = c.get("maxLength") {
                size.push(format!("max = {}", scalar_str(v)));
            }
            if !size.is_empty() {
                anns.push(format!("@Size({})", size.join(", ")));
                imports.insert("jakarta.validation.constraints.Size".into());
            }
            if let Some(p) = c.get("pattern") {
                if truthy(p) {
                    anns.push(format!("@Pattern(regexp = \"{}\")", java_string(&scalar_str(p))));
                    imports.insert("jakarta.validation.constraints.Pattern".into());
                }
            }
        }
        "integer" => {
            if let Some(v) = c.get("minimum") {
                anns.push(format!("@Min({})", scalar_str(v)));
                imports.insert("jakarta.validation.constraints.Min".into());
            }
            if let Some(v) = c.get("maximum") {
                anns.push(format!("@Max({})", scalar_str(v)));
                imports.insert("jakarta.validation.constraints.Max".into());
            }
        }
        "number" => {
            if let Some(v) = c.get("minimum") {
                anns.push(format!("@DecimalMin(\"{}\")", scalar_str(v)));
                imports.insert("jakarta.validation.constraints.DecimalMin".into());
            }
            if let Some(v) = c.get("maximum") {
                anns.push(format!("@DecimalMax(\"{}\")", scalar_str(v)));
                imports.insert("jakarta.validation.constraints.DecimalMax".into());
            }
        }
        "array" => {
            let mut size: Vec<String> = vec![];
            if let Some(v) = c.get("minItems") {
                size.push(format!("min = {}", scalar_str(v)));
            }
            if let Some(v) = c.get("maxItems") {
                size.push(format!("max = {}", scalar_str(v)));
            }
            if !size.is_empty() {
                anns.push(format!("@Size({})", size.join(", ")));
                imports.insert("jakarta.validation.constraints.Size".into());
            }
        }
        _ => {}
    }
    anns
}

pub fn emit_model(model: &Model, package: &str, command: &str) -> String {
    let mut imports: BTreeSet<String> = BTreeSet::new();
    let mut head: Vec<String> = vec![
        generated_header("java", command),
        String::new(),
        format!("package {package}.model;"),
        String::new(),
    ];
    if model.kind == "enum" {
        let mut body: Vec<String> = vec![];
        if let Some(d) = &model.description {
            body.push(format!("/** {} */", d.replace("*/", "* /")));
        }
        body.push(format!("public enum {} {{", model.name));
        body.push("    ".to_string() + &model.enum_values.join(", "));
        body.push("}".into());
        return head.into_iter().chain(body).collect::<Vec<_>>().join("\n") + "\n";
    }
    let mut comps: Vec<String> = vec![];
    for f in &model.fields {
        let anns = field_annotations(f, &mut imports);
        let jtype = java_type(&f.r#type, &mut imports);
        if anns.len() == 1 {
            comps.push(format!("        {} {} {},", anns[0], jtype, jname(&f.name)));
        } else {
            for a in &anns {
                comps.push(format!("        {a}"));
            }
            comps.push(format!("        {} {},", jtype, jname(&f.name)));
        }
    }
    let mut body: Vec<String> = vec![];
    if let Some(d) = &model.description {
        body.push(format!("/** {} */", d.replace("*/", "* /")));
    }
    body.push(format!("public record {}(", model.name));
    if !comps.is_empty() {
        body.push(comps.join("\n").trim_end_matches(',').to_string());
    }
    body.push(") {".into());
    body.push("}".into());
    for imp in &imports {
        head.push(format!("import {imp};"));
    }
    if !imports.is_empty() {
        head.push(String::new());
    }
    head.into_iter().chain(body).collect::<Vec<_>>().join("\n") + "\n"
}

fn param_annotation(p: &Param) -> String {
    match p.location.as_str() {
        "path" => format!("@PathParam(\"{}\")", p.name),
        "query" => format!("@QueryParam(\"{}\")", p.name),
        _ => format!("@HeaderParam(\"{}\")", p.name),
    }
}

pub fn emit_resource(tag: &str, endpoints: &[&Endpoint], package: &str, command: &str) -> String {
    let mut imports: BTreeSet<String> = [
        "jakarta.ws.rs.Consumes".to_string(),
        "jakarta.ws.rs.Path".to_string(),
        "jakarta.ws.rs.Produces".to_string(),
        "jakarta.ws.rs.core.MediaType".to_string(),
    ]
    .into_iter()
    .collect();
    let base = common_path_prefix(&endpoints.iter().map(|e| e.path.clone()).collect::<Vec<_>>());
    let mut lines: Vec<String> = vec![
        generated_header("java", command),
        String::new(),
        format!("package {package}.api;"),
        String::new(),
    ];
    let resource_name = format!("{}Resource", pascal(tag));
    let mut methods: Vec<String> = vec![];
    let mut eps: Vec<&Endpoint> = endpoints.to_vec();
    eps.sort_by(|a, b| (&a.path, &a.method).cmp(&(&b.path, &b.method)));
    for ep in &eps {
        imports.insert(format!("jakarta.ws.rs.{}", ep.method));
        let sub = if !base.is_empty() && ep.path.starts_with(&base) {
            ep.path[base.len()..].to_string()
        } else {
            ep.path.clone()
        };
        let mut ps: Vec<&Param> = ep.params.iter().collect();
        ps.sort_by(|a, b| {
            let ka = (a.location != "path", a.location != "query", a.name.clone());
            let kb = (b.location != "path", b.location != "query", b.name.clone());
            ka.cmp(&kb)
        });
        let mut args: Vec<String> = vec![];
        for p in &ps {
            imports.insert(format!("jakarta.ws.rs.{}Param", capitalize(&p.location)));
            args.push(format!(
                "{} {} {}",
                param_annotation(p),
                java_type(&p.r#type, &mut imports),
                jname(&p.name)
            ));
        }
        if let Some(rb) = &ep.request_body {
            args.push(format!("{} body", java_type(rb, &mut imports)));
        }
        let ret = if ep.response.is_none() {
            imports.insert("jakarta.ws.rs.core.Response".into());
            "Response".to_string()
        } else {
            java_type(ep.response.as_ref().unwrap(), &mut imports)
        };
        let mut m: Vec<String> = vec![format!("    @{}", ep.method)];
        if !sub.is_empty() {
            m.push(format!("    @Path(\"{sub}\")"));
        }
        if let Some(s) = &ep.summary {
            m.push(format!("    /** {} */", s.replace("*/", "* /")));
        }
        m.push(format!("    public {} {}({}) {{", ret, camel(&ep.operation_id), args.join(", ")));
        m.push(format!(
            "        throw new UnsupportedOperationException(\"TODO implement {}\");",
            ep.operation_id
        ));
        m.push("    }".into());
        methods.push(m.join("\n"));
    }
    let mut model_refs: Vec<Option<&TypeRef>> = vec![];
    for ep in &eps {
        model_refs.extend(ep.params.iter().map(|p| Some(&p.r#type)));
        model_refs.push(ep.request_body.as_ref());
        model_refs.push(ep.response.as_ref());
    }
    imports.extend(model_imports(package, &model_refs));
    for imp in &imports {
        lines.push(format!("import {imp};"));
    }
    lines.push(String::new());
    lines.push(format!("@Path(\"{}\")", if base.is_empty() { "/" } else { &base }));
    lines.push("@Produces(MediaType.APPLICATION_JSON)".into());
    lines.push("@Consumes(MediaType.APPLICATION_JSON)".into());
    lines.push(format!("public class {resource_name} {{"));
    lines.push(String::new());
    lines.push(methods.join("\n\n"));
    lines.push("}".into());
    lines.join("\n") + "\n"
}

pub fn emit_producer(channel: &str, flows: &[&Flow], package: &str, command: &str) -> String {
    let mut imports: BTreeSet<String> = [
        "java.util.concurrent.CompletionStage".to_string(),
        "jakarta.enterprise.context.ApplicationScoped".to_string(),
        "org.eclipse.microprofile.reactive.messaging.Channel".to_string(),
        "org.eclipse.microprofile.reactive.messaging.Emitter".to_string(),
    ]
    .into_iter()
    .collect();
    let mut emitter_names: Vec<(String, String)> = vec![];
    let mut methods: Vec<String> = vec![];
    for f in flows {
        let ptype = match &f.payload {
            Some(p) => java_type(p, &mut imports),
            None => "String".to_string(),
        };
        let ename = match emitter_names.iter().find(|(p, _)| *p == ptype) {
            Some((_, e)) => e.clone(),
            None => {
                let e = format!("{}Emitter", camel(&ptype));
                emitter_names.push((ptype.clone(), e.clone()));
                e
            }
        };
        methods.push(format!(
            "    public CompletionStage<Void> {}({} event) {{\n        return {}.send(event);\n    }}",
            camel(&f.name),
            ptype,
            ename
        ));
    }
    imports.extend(model_imports(package, &flows.iter().map(|f| f.payload.as_ref()).collect::<Vec<_>>()));
    let mut lines: Vec<String> = vec![
        generated_header("java", command),
        String::new(),
        format!("package {package}.messaging;"),
        String::new(),
    ];
    for imp in &imports {
        lines.push(format!("import {imp};"));
    }
    lines.push(String::new());
    lines.push(format!(
        "/** Publishes on channel `{channel}` (topic: {}). */",
        flows[0].topic.as_deref().unwrap_or(channel)
    ));
    lines.push("@ApplicationScoped".into());
    lines.push(format!("public class {}Producer {{", pascal(channel)));
    lines.push(String::new());
    let emitters: Vec<String> = emitter_names
        .iter()
        .map(|(ptype, ename)| format!("    @Channel(\"{channel}\")\n    Emitter<{ptype}> {ename};"))
        .collect();
    lines.push(emitters.join("\n\n"));
    lines.push(String::new());
    lines.push(methods.join("\n\n"));
    lines.push("}".into());
    lines.join("\n") + "\n"
}

pub fn emit_consumer(channel: &str, flows: &[&Flow], package: &str, command: &str) -> String {
    let mut imports: BTreeSet<String> = [
        "java.util.concurrent.CompletableFuture".to_string(),
        "java.util.concurrent.CompletionStage".to_string(),
        "jakarta.enterprise.context.ApplicationScoped".to_string(),
        "org.eclipse.microprofile.reactive.messaging.Incoming".to_string(),
    ]
    .into_iter()
    .collect();
    let mut methods: Vec<String> = vec![];
    for f in flows {
        let ptype = match &f.payload {
            Some(p) => java_type(p, &mut imports),
            None => "String".to_string(),
        };
        methods.push(format!(
            "    @Incoming(\"{channel}\")\n    public CompletionStage<Void> {}({} event) {{\n        return handle{}(event);\n    }}",
            camel(&f.name),
            ptype,
            pascal(&f.name)
        ));
    }
    let mut handles: Vec<String> = vec![];
    for f in flows {
        let ptype = match &f.payload {
            Some(p) => java_type(p, &mut imports),
            None => "String".to_string(),
        };
        handles.push(format!(
            "    protected CompletionStage<Void> handle{}({} event) {{\n        // TODO implement business logic for {}\n        return CompletableFuture.completedFuture(null);\n    }}",
            pascal(&f.name),
            ptype,
            f.name
        ));
    }
    imports.extend(model_imports(package, &flows.iter().map(|f| f.payload.as_ref()).collect::<Vec<_>>()));
    let mut lines: Vec<String> = vec![
        generated_header("java", command),
        String::new(),
        format!("package {package}.messaging;"),
        String::new(),
    ];
    for imp in &imports {
        lines.push(format!("import {imp};"));
    }
    lines.push(String::new());
    lines.push(format!(
        "/** Consumes from channel `{channel}` (topic: {}). */",
        flows[0].topic.as_deref().unwrap_or(channel)
    ));
    lines.push("@ApplicationScoped".into());
    lines.push(format!("public class {}Consumer {{", pascal(channel)));
    lines.push(String::new());
    lines.push(methods.join("\n\n"));
    lines.push(String::new());
    lines.push(handles.join("\n\n"));
    lines.push("}".into());
    lines.join("\n") + "\n"
}

const POM_TMPL: &str = r#"@HEADER@
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
  <groupId>@GROUP_ID@</groupId>
  <artifactId>@ARTIFACT@</artifactId>
  <version>1.0.0-SNAPSHOT</version>
  <properties>
    <maven.compiler.release>21</maven.compiler.release>
    <quarkus.platform.group-id>io.quarkus.platform</quarkus.platform.group-id>
    <quarkus.platform.artifact-id>quarkus-bom</quarkus.platform.artifact-id>
    <quarkus.platform.version>@QUARKUS_VERSION@</quarkus.platform.version>
  </properties>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>${quarkus.platform.group-id}</groupId>
        <artifactId>${quarkus.platform.artifact-id}</artifactId>
        <version>${quarkus.platform.version}</version>
        <type>pom</type>
        <scope>import</scope>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
@DEP_XML@
  </dependencies>
  <build>
    <plugins>
      <plugin>
        <groupId>${quarkus.platform.group-id}</groupId>
        <artifactId>quarkus-maven-plugin</artifactId>
        <version>${quarkus.platform.version}</version>
        <extensions>true</extensions>
        <executions>
          <execution>
            <goals>
              <goal>build</goal>
              <goal>generate-code</goal>
            </goals>
          </execution>
        </executions>
      </plugin>
    </plugins>
  </build>
</project>
"#;

pub fn emit_pom(group_id: &str, artifact: &str, has_flows: bool, command: &str) -> String {
    let mut deps = vec!["quarkus-rest-jackson", "quarkus-hibernate-validator"];
    if has_flows {
        deps.push("quarkus-smallrye-reactive-messaging-kafka");
    }
    let dep_xml = deps
        .iter()
        .map(|d| {
            format!(
                "      <dependency>\n        <groupId>io.quarkus</groupId>\n        <artifactId>{d}</artifactId>\n      </dependency>"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    POM_TMPL
        .replace("@HEADER@", &generated_header("xml", command))
        .replace("@GROUP_ID@", group_id)
        .replace("@ARTIFACT@", artifact)
        .replace("@QUARKUS_VERSION@", QUARKUS_VERSION)
        .replace("@DEP_XML@", &dep_xml)
}

pub fn emit_application_properties(
    channels: &std::collections::BTreeMap<String, crate::ir::ChannelFlows>,
    command: &str,
) -> String {
    let mut lines: Vec<String> = vec![generated_header("properties", command), String::new()];
    for (channel, info) in channels {
        let topic = info.topic.as_deref().unwrap_or(channel);
        for (action, direction) in [("send", "outgoing"), ("receive", "incoming")] {
            let non_empty = if action == "send" { !info.send.is_empty() } else { !info.receive.is_empty() };
            if non_empty {
                lines.push(format!(
                    "mp.messaging.{direction}.{channel}.connector=smallrye-kafka"
                ));
                lines.push(format!("mp.messaging.{direction}.{channel}.topic={topic}"));
            }
        }
    }
    lines.join("\n") + "\n"
}

/// Generate the full Quarkus project into out_dir. Returns written file paths.
pub fn emit_quarkus(
    ir: &ApiIR,
    out_dir: &Path,
    package: &str,
    group_id: &str,
    artifact: Option<&str>,
    command: &str,
) -> Result<Vec<PathBuf>> {
    let artifact = artifact
        .map(|s| s.to_string())
        .unwrap_or_else(|| ir.api_name.to_lowercase().replace(' ', "-"));
    let pkg_path = package.replace('.', "/");
    let mut written: Vec<PathBuf> = vec![];
    macro_rules! w {
        ($rel:expr, $content:expr) => {
            written.push(write_file(out_dir, $rel, &$content)?)
        };
    }

    w!("pom.xml", emit_pom(group_id, &artifact, !ir.flows.is_empty(), command));
    w!(
        "src/main/resources/application.properties",
        emit_application_properties(&ir.flows_by_channel(), command)
    );

    for (name, model) in &ir.models {
        w!(
            &format!("src/main/java/{pkg_path}/model/{name}.java"),
            emit_model(model, package, command)
        );
    }

    for tag in ir.tags() {
        w!(
            &format!("src/main/java/{pkg_path}/api/{}Resource.java", pascal(&tag)),
            emit_resource(&tag, &ir.endpoints_for_tag(&tag), package, command)
        );
    }

    for (channel, info) in ir.flows_by_channel() {
        if !info.send.is_empty() {
            let flows: Vec<&Flow> = info.send.iter().collect();
            w!(
                &format!("src/main/java/{pkg_path}/messaging/{}Producer.java", pascal(&channel)),
                emit_producer(&channel, &flows, package, command)
            );
        }
        if !info.receive.is_empty() {
            let flows: Vec<&Flow> = info.receive.iter().collect();
            w!(
                &format!("src/main/java/{pkg_path}/messaging/{}Consumer.java", pascal(&channel)),
                emit_consumer(&channel, &flows, package, command)
            );
        }
    }

    w!(
        "README.md",
        format!(
            r#"# {api_name} — Quarkus skeleton (GENERATED)

Generated from OpenAPI/AsyncAPI specs (version {version}). **Do not edit; regenerate.**

- `model/` — data model records/enums with Bean Validation constraints.
- `api/` — JAX-RS resources (one per OpenAPI tag). Method bodies are `TODO` stubs.
- `messaging/` — SmallRye Reactive Messaging producers/consumers (one per AsyncAPI channel).
- `application.properties` — Kafka channel/topic wiring from the AsyncAPI bindings.

Hand-written code must live outside these packages (or in files not owned by the
generator); regeneration replaces this whole tree.
"#,
            api_name = ir.api_name,
            version = ir.version
        )
    );
    Ok(written)
}
