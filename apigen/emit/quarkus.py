"""Emit a Java Quarkus service skeleton from the ApiIR.

Layout (fully regenerated on each run — spec is the source of truth):
    pom.xml
    src/main/resources/application.properties   (messaging channels config)
    src/main/java/<pkg>/model/*.java             (records + enums, Bean Validation)
    src/main/java/<pkg>/api/*Resource.java       (JAX-RS resources, one per OpenAPI tag)
    src/main/java/<pkg>/messaging/*.java         (producers/consumers per AsyncAPI channel)
"""

import os

from ..ir import Model
from ..naming import camel, pascal
from .common import common_path_prefix, generated_header, write_file

JAVA_KEYWORDS = {
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class",
    "const", "continue", "default", "do", "double", "else", "enum", "extends", "final",
    "finally", "float", "for", "goto", "if", "implements", "import", "instanceof", "int",
    "interface", "long", "native", "new", "package", "private", "protected", "public",
    "return", "short", "static", "strictfp", "super", "switch", "synchronized", "this",
    "throw", "throws", "transient", "try", "void", "volatile", "while", "true", "false", "null",
}

_QUARKUS_VERSION = "3.15.1"


def _jname(name):
    n = camel(name)
    return n + "_" if n in JAVA_KEYWORDS else n


def _collect_refs(tref, out):
    if tref is None:
        return
    if tref.is_ref():
        out.add(tref.ref)
    _collect_refs(tref.item, out)
    _collect_refs(tref.value, out)


def _model_imports(package, trefs):
    refs = set()
    for t in trefs:
        _collect_refs(t, refs)
    return {"%s.model.%s" % (package, r) for r in refs}


def java_type(tref, imports):
    """Map a TypeRef to a Java type; register needed imports in `imports`."""
    k = tref.kind
    if k == "ref":
        return tref.ref
    if k == "string":
        f = tref.format
        if f == "date":
            imports.add("java.time.LocalDate")
            return "LocalDate"
        if f == "date-time":
            imports.add("java.time.OffsetDateTime")
            return "OffsetDateTime"
        if f == "uuid":
            imports.add("java.util.UUID")
            return "UUID"
        if f in ("byte", "binary"):
            return "byte[]"
        return "String"
    if k == "integer":
        return "Integer" if tref.format == "int32" else "Long"
    if k == "number":
        return "Float" if tref.format == "float" else "Double"
    if k == "boolean":
        return "Boolean"
    if k == "array":
        imports.add("java.util.List")
        return "List<%s>" % java_type(tref.item, imports)
    if k == "map":
        imports.add("java.util.Map")
        return "Map<String, %s>" % java_type(tref.value, imports)
    if k == "object":
        imports.add("java.util.Map")
        return "Map<String, Object>"
    return "Object"


def _java_string(value):
    return str(value).replace("\\", "\\\\").replace('"', '\\"')


def field_annotations(field, imports):
    anns = []
    if field.required:
        anns.append("@NotNull")
        imports.add("jakarta.validation.constraints.NotNull")
    c = field.constraints
    t = field.type
    if t.kind == "string":
        if c.get("format") == "email":
            anns.append("@Email")
            imports.add("jakarta.validation.constraints.Email")
        size = []
        if "minLength" in c:
            size.append("min = %s" % c["minLength"])
        if "maxLength" in c:
            size.append("max = %s" % c["maxLength"])
        if size:
            anns.append("@Size(%s)" % ", ".join(size))
            imports.add("jakarta.validation.constraints.Size")
        if c.get("pattern"):
            anns.append('@Pattern(regexp = "%s")' % _java_string(c["pattern"]))
            imports.add("jakarta.validation.constraints.Pattern")
    elif t.kind == "integer":
        if "minimum" in c:
            anns.append("@Min(%s)" % c["minimum"])
            imports.add("jakarta.validation.constraints.Min")
        if "maximum" in c:
            anns.append("@Max(%s)" % c["maximum"])
            imports.add("jakarta.validation.constraints.Max")
    elif t.kind == "number":
        if "minimum" in c:
            anns.append('@DecimalMin("%s")' % c["minimum"])
            imports.add("jakarta.validation.constraints.DecimalMin")
        if "maximum" in c:
            anns.append('@DecimalMax("%s")' % c["maximum"])
            imports.add("jakarta.validation.constraints.DecimalMax")
    elif t.kind == "array":
        size = []
        if "minItems" in c:
            size.append("min = %s" % c["minItems"])
        if "maxItems" in c:
            size.append("max = %s" % c["maxItems"])
        if size:
            anns.append("@Size(%s)" % ", ".join(size))
            imports.add("jakarta.validation.constraints.Size")
    return anns


def emit_model(model: Model, package: str, command: str) -> str:
    imports = set()
    head = [generated_header("java", command), "", "package %s.model;" % package, ""]
    if model.kind == "enum":
        body = []
        if model.description:
            body.append("/** %s */" % model.description.replace("*/", "* /"))
        body.append("public enum %s {" % model.name)
        body.append("    " + ", ".join(model.enum_values))
        body.append("}")
        return "\n".join(head + body) + "\n"
    comps = []
    for f in model.fields:
        anns = field_annotations(f, imports)
        jtype = java_type(f.type, imports)
        if len(anns) == 1:
            comps.append("        %s %s %s," % (anns[0], jtype, _jname(f.name)))
        else:
            for a in anns:
                comps.append("        %s" % a)
            comps.append("        %s %s," % (jtype, _jname(f.name)))
    body = []
    if model.description:
        body.append("/** %s */" % model.description.replace("*/", "* /"))
    body.append("public record %s(" % model.name)
    if comps:
        body.append("\n".join(comps).rstrip(","))
    body.append(") {")
    body.append("}")
    for imp in sorted(imports):
        head.append("import %s;" % imp)
    if imports:
        head.append("")
    return "\n".join(head + body) + "\n"


def _param_annotation(p, imports):
    if p.location == "path":
        return '@PathParam("%s")' % p.name
    if p.location == "query":
        return '@QueryParam("%s")' % p.name
    return '@HeaderParam("%s")' % p.name


def emit_resource(tag, endpoints, package, command):
    imports = {"jakarta.ws.rs.Consumes", "jakarta.ws.rs.Path", "jakarta.ws.rs.Produces",
               "jakarta.ws.rs.core.MediaType"}
    base = common_path_prefix([ep.path for ep in endpoints])
    lines = [generated_header("java", command), "",
             "package %s.api;" % package, ""]
    resource_name = pascal(tag) + "Resource"
    methods = []
    for ep in sorted(endpoints, key=lambda e: (e.path, e.method)):
        imports.add("jakarta.ws.rs.%s" % ep.method)
        sub = ep.path[len(base):] if base and ep.path.startswith(base) else ep.path
        args = []
        for p in sorted(ep.params, key=lambda x: (x.location != "path", x.location != "query", x.name)):
            imports.add("jakarta.ws.rs.%sParam" % p.location.capitalize())
            args.append("%s %s %s" % (_param_annotation(p, imports),
                                      java_type(p.type, imports), _jname(p.name)))
        if ep.request_body is not None:
            args.append("%s body" % java_type(ep.request_body, imports))
        if ep.response is None:
            imports.add("jakarta.ws.rs.core.Response")
            ret = "Response"
        else:
            ret = java_type(ep.response, imports)
        m = ["    @%s" % ep.method]
        if sub:
            m.append('    @Path("%s")' % sub)
        if ep.summary:
            m.append("    /** %s */" % ep.summary.replace("*/", "* /"))
        m.append("    public %s %s(%s) {" % (ret, camel(ep.operation_id), ", ".join(args)))
        m.append('        throw new UnsupportedOperationException("TODO implement %s");'
                 % ep.operation_id)
        m.append("    }")
        methods.append("\n".join(m))
    model_refs = []
    for ep in endpoints:
        model_refs += [p.type for p in ep.params]
        model_refs += [ep.request_body, ep.response]
    imports |= _model_imports(package, model_refs)
    for imp in sorted(imports):
        lines.append("import %s;" % imp)
    lines.append("")
    lines.append('@Path("%s")' % (base or "/"))
    lines.append("@Produces(MediaType.APPLICATION_JSON)")
    lines.append("@Consumes(MediaType.APPLICATION_JSON)")
    lines.append("public class %s {" % resource_name)
    lines.append("")
    lines.append("\n\n".join(methods))
    lines.append("}")
    return "\n".join(lines) + "\n"


def emit_producer(channel, flows, package, command):
    imports = {"java.util.concurrent.CompletionStage",
               "jakarta.enterprise.context.ApplicationScoped",
               "org.eclipse.microprofile.reactive.messaging.Channel",
               "org.eclipse.microprofile.reactive.messaging.Emitter"}
    emitter_names = {}
    methods = []
    for f in flows:
        ptype = java_type(f.payload, imports) if f.payload else "String"
        ename = emitter_names.setdefault(ptype, camel(ptype) + "Emitter")
        methods.append(
            "    public CompletionStage<Void> %s(%s event) {\n"
            "        return %s.send(event);\n"
            "    }" % (camel(f.name), ptype, ename))
    imports |= _model_imports(package, [f.payload for f in flows])
    lines = [generated_header("java", command), "",
             "package %s.messaging;" % package, ""]
    for imp in sorted(imports):
        lines.append("import %s;" % imp)
    lines += ["", "/** Publishes on channel `%s` (topic: %s). */" % (channel, flows[0].topic or channel),
              "@ApplicationScoped",
              "public class %sProducer {" % pascal(channel), ""]
    emitters = ['    @Channel("%s")\n    Emitter<%s> %s;' % (channel, ptype, ename)
                for ptype, ename in emitter_names.items()]
    lines.append("\n\n".join(emitters))
    lines.append("")
    lines.append("\n\n".join(methods))
    lines.append("}")
    return "\n".join(lines) + "\n"


def emit_consumer(channel, flows, package, command):
    imports = {"java.util.concurrent.CompletableFuture", "java.util.concurrent.CompletionStage",
               "jakarta.enterprise.context.ApplicationScoped",
               "org.eclipse.microprofile.reactive.messaging.Incoming"}
    methods = []
    for f in flows:
        ptype = java_type(f.payload, imports) if f.payload else "String"
        methods.append(
            '    @Incoming("%s")\n'
            "    public CompletionStage<Void> %s(%s event) {\n"
            "        return handle%s(event);\n"
            "    }" % (channel, camel(f.name), ptype, pascal(f.name)))
    handles = []
    for f in flows:
        ptype = java_type(f.payload, imports) if f.payload else "String"
        handles.append(
            "    protected CompletionStage<Void> handle%s(%s event) {\n"
            "        // TODO implement business logic for %s\n"
            "        return CompletableFuture.completedFuture(null);\n"
            "    }" % (pascal(f.name), ptype, f.name))
    imports |= _model_imports(package, [f.payload for f in flows])
    lines = [generated_header("java", command), "",
             "package %s.messaging;" % package, ""]
    for imp in sorted(imports):
        lines.append("import %s;" % imp)
    lines += ["", "/** Consumes from channel `%s` (topic: %s). */" % (channel, flows[0].topic or channel),
              "@ApplicationScoped",
              "public class %sConsumer {" % pascal(channel), ""]
    lines.append("\n\n".join(methods))
    lines.append("")
    lines.append("\n\n".join(handles))
    lines.append("}")
    return "\n".join(lines) + "\n"


def emit_pom(group_id, artifact, has_flows, command):
    deps = ["quarkus-rest-jackson", "quarkus-hibernate-validator"]
    if has_flows:
        deps.append("quarkus-smallrye-reactive-messaging-kafka")
    dep_xml = "\n".join(
        "      <dependency>\n        <groupId>io.quarkus</groupId>\n"
        "        <artifactId>%s</artifactId>\n      </dependency>" % d for d in deps)
    return f"""{generated_header("xml", command)}
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
  <groupId>{group_id}</groupId>
  <artifactId>{artifact}</artifactId>
  <version>1.0.0-SNAPSHOT</version>
  <properties>
    <maven.compiler.release>21</maven.compiler.release>
    <quarkus.platform.group-id>io.quarkus.platform</quarkus.platform.group-id>
    <quarkus.platform.artifact-id>quarkus-bom</quarkus.platform.artifact-id>
    <quarkus.platform.version>{_QUARKUS_VERSION}</quarkus.platform.version>
  </properties>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>${{quarkus.platform.group-id}}</groupId>
        <artifactId>${{quarkus.platform.artifact-id}}</artifactId>
        <version>${{quarkus.platform.version}}</version>
        <type>pom</type>
        <scope>import</scope>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
{dep_xml}
  </dependencies>
  <build>
    <plugins>
      <plugin>
        <groupId>${{quarkus.platform.group-id}}</groupId>
        <artifactId>quarkus-maven-plugin</artifactId>
        <version>${{quarkus.platform.version}}</version>
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
"""


def emit_application_properties(channels, command):
    lines = [generated_header("properties", command), ""]
    for channel, info in sorted(channels.items()):
        topic = info["topic"] or channel
        for action in ("send", "receive"):
            if info[action]:
                direction = "outgoing" if action == "send" else "incoming"
                lines.append("mp.messaging.%s.%s.connector=smallrye-kafka" % (direction, channel))
                lines.append("mp.messaging.%s.%s.topic=%s" % (direction, channel, topic))
    return "\n".join(lines) + "\n"


def emit_quarkus(ir, out_dir, package="com.example.api", group_id="com.example",
                 artifact=None, command="python -m apigen ..."):
    """Generate the full Quarkus project into out_dir. Returns written file paths."""
    artifact = artifact or ir.api_name.lower().replace(" ", "-")
    pkg_path = package.replace(".", "/")
    written = []
    w = lambda rel, content: written.append(write_file(out_dir, rel, content))

    w("pom.xml", emit_pom(group_id, artifact, bool(ir.flows), command))
    w("src/main/resources/application.properties",
      emit_application_properties(ir.flows_by_channel(), command))

    for name in sorted(ir.models):
        model = ir.models[name]
        w("src/main/java/%s/model/%s.java" % (pkg_path, name),
          emit_model(model, package, command))

    for tag in ir.tags():
        w("src/main/java/%s/api/%sResource.java" % (pkg_path, pascal(tag)),
          emit_resource(tag, ir.endpoints_for_tag(tag), package, command))

    for channel, info in sorted(ir.flows_by_channel().items()):
        if info["send"]:
            w("src/main/java/%s/messaging/%sProducer.java" % (pkg_path, pascal(channel)),
              emit_producer(channel, info["send"], package, command))
        if info["receive"]:
            w("src/main/java/%s/messaging/%sConsumer.java" % (pkg_path, pascal(channel)),
              emit_consumer(channel, info["receive"], package, command))

    w("README.md", f"""# {ir.api_name} — Quarkus skeleton (GENERATED)

Generated from OpenAPI/AsyncAPI specs (version {ir.version}). **Do not edit; regenerate.**

- `model/` — data model records/enums with Bean Validation constraints.
- `api/` — JAX-RS resources (one per OpenAPI tag). Method bodies are `TODO` stubs.
- `messaging/` — SmallRye Reactive Messaging producers/consumers (one per AsyncAPI channel).
- `application.properties` — Kafka channel/topic wiring from the AsyncAPI bindings.

Hand-written code must live outside these packages (or in files not owned by the
generator); regeneration replaces this whole tree.
""")
    return written
