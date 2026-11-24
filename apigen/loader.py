"""Load OpenAPI 3.x and AsyncAPI 2.x/3.x documents into the ApiIR.

Supports local pointers (#/...) and external file refs (./other.yaml#/...),
which EA exports use when the AsyncAPI file references the shared data model
defined in the OpenAPI file.
"""

import os

import yaml

from .ir import ApiIR, Endpoint, Field, Flow, Model, Param, TypeRef
from .naming import camel, pascal, snake

HTTP_METHODS = ("get", "put", "post", "delete", "patch", "head", "options", "trace")


class SpecContext:
    """Caches parsed documents and resolves $ref pointers across files."""

    def __init__(self):
        self._docs = {}

    def load_doc(self, path):
        ap = os.path.abspath(path)
        if ap not in self._docs:
            with open(ap, "r", encoding="utf-8") as fh:
                self._docs[ap] = yaml.safe_load(fh)
        return self._docs[ap]

    def resolve_ref(self, ref, current_file):
        """Return (target_object, file_the_target_lives_in)."""
        if ref.startswith("#"):
            return _pointer(self.load_doc(current_file), ref[1:]), current_file
        file_part, _, ptr = ref.partition("#")
        target_file = os.path.join(os.path.dirname(os.path.abspath(current_file)), file_part)
        doc = self.load_doc(target_file)
        if ptr:
            return _pointer(doc, "/" + ptr), target_file
        return doc, target_file


def _pointer(doc, ptr):
    cur = doc
    for seg in ptr.split("/"):
        if seg == "":
            continue
        seg = seg.replace("~1", "/").replace("~0", "~")
        cur = cur[seg]
    return cur


def _ref_name(ref):
    return ref.split("/")[-1]


# --------------------------------------------------------------------------
# schema -> TypeRef
# --------------------------------------------------------------------------

def schema_to_ref(ctx, file, schema):
    if not isinstance(schema, dict):
        return TypeRef("any")
    if "$ref" in schema:
        return TypeRef("ref", ref=_ref_name(schema["$ref"]))
    if "allOf" in schema and len(schema["allOf"]) == 1 and "$ref" in schema["allOf"][0]:
        return schema_to_ref(ctx, file, schema["allOf"][0])
    if "enum" in schema:
        # inline enum used directly as a property: caller registers a model for it
        return TypeRef("any")
    t = schema.get("type")
    if isinstance(t, list):
        t = next((x for x in t if x != "null"), None)
    if t == "array":
        return TypeRef("array", item=schema_to_ref(ctx, file, schema.get("items") or {}))
    if t == "object" or "properties" in schema or "additionalProperties" in schema:
        ap = schema.get("additionalProperties")
        if isinstance(ap, dict):
            return TypeRef("map", value=schema_to_ref(ctx, file, ap))
        if ap is True or (t == "object" and "properties" not in schema):
            return TypeRef("map", value=TypeRef("any"))
        return TypeRef("object")  # inline object; build_model registers a named model
    if t == "integer":
        return TypeRef("integer", format=schema.get("format"))
    if t == "number":
        return TypeRef("number", format=schema.get("format"))
    if t == "boolean":
        return TypeRef("boolean")
    if t == "string":
        return TypeRef("string", format=schema.get("format"))
    return TypeRef("any")


# --------------------------------------------------------------------------
# models
# --------------------------------------------------------------------------

_CONSTRAINT_KEYS = (
    "minLength", "maxLength", "pattern", "minimum", "maximum",
    "exclusiveMinimum", "exclusiveMaximum", "minItems", "maxItems",
    "multipleOf", "default", "format",
)


def property_ref(ctx, file, models, parent_name, prop_name, schema):
    """Return a TypeRef for a property, registering inline object/enum models."""
    if isinstance(schema, dict):
        if "$ref" in schema:
            name = _ref_name(schema["$ref"])
            if name not in models:
                target, target_file = ctx.resolve_ref(schema["$ref"], file)
                build_model(ctx, target_file, models, name, target)
            return TypeRef("ref", ref=name)
        if "allOf" in schema and not schema.get("properties"):
            merged = {}
            for part in schema["allOf"]:
                resolved = part
                if isinstance(part, dict) and "$ref" in part:
                    resolved, _ = ctx.resolve_ref(part["$ref"], file)
                if isinstance(resolved, dict):
                    merged.update(resolved)
            return property_ref(ctx, file, models, parent_name, prop_name, merged)
        if "enum" in schema:
            mname = parent_name + pascal(prop_name)
            if mname not in models:
                models[mname] = Model(
                    name=mname, kind="enum",
                    description=schema.get("description"),
                    enum_values=[str(v) for v in schema["enum"]],
                )
            return TypeRef("ref", ref=mname)
        if schema.get("type") == "object" and schema.get("properties"):
            mname = parent_name + pascal(prop_name)
            if mname not in models:
                build_model(ctx, file, models, mname, schema)
            return TypeRef("ref", ref=mname)
    return schema_to_ref(ctx, file, schema)


def register_refs(ctx, file, models, schema):
    """Recursively register a model for every $ref found in a schema tree."""
    if isinstance(schema, dict):
        if "$ref" in schema:
            name = _ref_name(schema["$ref"])
            if name not in models:
                target, tfile = ctx.resolve_ref(schema["$ref"], file)
                build_model(ctx, tfile, models, name, target)
        else:
            for v in schema.values():
                register_refs(ctx, file, models, v)
    elif isinstance(schema, list):
        for v in schema:
            register_refs(ctx, file, models, v)


def build_model(ctx, file, models, name, schema):
    if name in models or not isinstance(schema, dict):
        return models.get(name)
    if "$ref" in schema:
        target, target_file = ctx.resolve_ref(schema["$ref"], file)
        return build_model(ctx, target_file, models, _ref_name(schema["$ref"]), target)
    if "allOf" in schema:
        props, required, desc = {}, [], schema.get("description")
        for part in schema["allOf"]:
            resolved = part
            if isinstance(part, dict) and "$ref" in part:
                resolved, _ = ctx.resolve_ref(part["$ref"], file)
                rname = _ref_name(part["$ref"])
                if rname not in models:
                    build_model(ctx, file, models, rname, resolved)
            if isinstance(resolved, dict):
                props.update(resolved.get("properties") or {})
                required += [r for r in (resolved.get("required") or []) if r not in required]
                desc = desc or resolved.get("description")
        schema = {"type": "object", "properties": props, "required": required, "description": desc}
    if "enum" in schema:
        models[name] = Model(name=name, kind="enum", description=schema.get("description"),
                             enum_values=[str(v) for v in schema["enum"]])
        return models[name]
    props = schema.get("properties") or {}
    required = set(schema.get("required") or [])
    fields = []
    for pname, pschema in props.items():
        pschema = pschema if isinstance(pschema, dict) else {}
        tref = property_ref(ctx, file, models, name, pname, pschema)
        constraints = {k: pschema[k] for k in _CONSTRAINT_KEYS if k in pschema}
        fields.append(Field(name=pname, type=tref, required=pname in required,
                            description=pschema.get("description"), constraints=constraints))
    models[name] = Model(name=name, kind="object", description=schema.get("description"), fields=fields)
    return models[name]


def load_schemas(ctx, file, doc, models):
    comps = (doc.get("components") or {}).get("schemas") or {}
    for name, schema in comps.items():
        build_model(ctx, file, models, name, schema)


# --------------------------------------------------------------------------
# OpenAPI
# --------------------------------------------------------------------------

def _content_schema(content):
    if not isinstance(content, dict):
        return None
    for mt in ("application/json", "application/*+json", "*/*"):
        if mt in content:
            return (content[mt] or {}).get("schema")
    for mt, entry in content.items():
        if "json" in mt:
            return (entry or {}).get("schema")
    return None


def load_openapi(ctx, path, models):
    doc = ctx.load_doc(path)
    load_schemas(ctx, path, doc, models)
    endpoints = []
    for path_str, item in (doc.get("paths") or {}).items():
        if not isinstance(item, dict):
            continue
        shared_params = item.get("parameters") or []
        for method, op in item.items():
            if method not in HTTP_METHODS or not isinstance(op, dict):
                continue
            op_id = op.get("operationId") or snake(method + " " + path_str)
            params = []
            merged = {(p.get("name"), p.get("in")): p for p in shared_params if isinstance(p, dict)}
            merged.update({(p.get("name"), p.get("in")): p for p in op.get("parameters") or [] if isinstance(p, dict)})
            for (pname, pin), p in sorted(merged.items()):
                pschema = p.get("schema") or {}
                register_refs(ctx, path, models, pschema)
                params.append(Param(name=pname, location=pin,
                                    type=schema_to_ref(ctx, path, pschema),
                                    required=bool(p.get("required")) or pin == "path",
                                    description=p.get("description")))
            body = None
            rb = op.get("requestBody") or {}
            bs = _content_schema(rb.get("content"))
            if bs is not None:
                register_refs(ctx, path, models, bs)
                body = schema_to_ref(ctx, path, bs)
            responses = op.get("responses") or {}
            status, rschema, eschema = "200", None, None
            for code in sorted(responses):
                s = _content_schema((responses[code] or {}).get("content"))
                if s is not None:
                    register_refs(ctx, path, models, s)
                if code.startswith("2") and rschema is None:
                    status, rschema = code, s
                elif code.startswith(("4", "5")) and eschema is None:
                    eschema = s
            resp = schema_to_ref(ctx, path, rschema) if rschema else None
            err = schema_to_ref(ctx, path, eschema) if eschema else None
            tag = (op.get("tags") or ["default"])[0]
            endpoints.append(Endpoint(method=method.upper(), path=path_str, operation_id=op_id,
                                      tag=tag, summary=op.get("summary") or op.get("description"),
                                      params=params, request_body=body, response=resp,
                                      response_status=status, error_response=err))
    return doc, endpoints


# --------------------------------------------------------------------------
# AsyncAPI
# --------------------------------------------------------------------------

def _channel_topic(channel_def, address):
    bindings = (channel_def or {}).get("bindings") or {}
    kafka = bindings.get("kafka") or {}
    return kafka.get("topic") or address


def _message_payload(ctx, file, models, msg_ref, inline_msg):
    msg = inline_msg or {}
    if msg_ref:
        msg, file = ctx.resolve_ref(msg_ref, file)
    seen = set()
    while (isinstance(msg, dict) and "$ref" in msg and "payload" not in msg
           and id(msg) not in seen):
        seen.add(id(msg))
        msg, file = ctx.resolve_ref(msg["$ref"], file)
    payload = msg.get("payload")
    if payload is None:
        return None, msg.get("name")
    if isinstance(payload, dict) and "$ref" in payload:
        name = _ref_name(payload["$ref"])
        if name not in models:
            target, tfile = ctx.resolve_ref(payload["$ref"], file)
            build_model(ctx, tfile, models, name, target)
        return TypeRef("ref", ref=name), msg.get("name") or name
    name = msg.get("name") or "Message"
    mname = pascal(name) + "Payload"
    if mname not in models:
        build_model(ctx, file, models, mname, payload)
    return TypeRef("ref", ref=mname), msg.get("name") or name


def load_asyncapi(ctx, path, models):
    doc = ctx.load_doc(path)
    load_schemas(ctx, path, doc, models)
    version = str(doc.get("asyncapi", "3.0.0"))
    flows = []
    if version.startswith("3"):
        channels = doc.get("channels") or {}
        chan_info = {}
        for cname, cdef in channels.items():
            address = (cdef or {}).get("address") or cname
            chan_info[cname] = {"address": address, "topic": _channel_topic(cdef, address)}
        for op_id, op in (doc.get("operations") or {}).items():
            chan_ref = (op.get("channel") or {}).get("$ref", "")
            cname = _ref_name(chan_ref)
            info = chan_info.get(cname, {"address": cname, "topic": cname})
            msgs = op.get("messages") or []
            payload, mname = None, None
            if msgs:
                payload, mname = _message_payload(ctx, path, models, msgs[0].get("$ref"), msgs[0])
            elif (op.get("channel") or {}):
                pass
            if payload is None:
                chan = channels.get(cname) or {}
                cmessages = chan.get("messages") or {}
                if cmessages:
                    first = next(iter(cmessages.values()))
                    payload, mname = _message_payload(ctx, path, models, None, first)
            flows.append(Flow(name=op_id, action=op.get("action", "receive"),
                              channel=info["address"], topic=info["topic"],
                              message_name=mname, payload=payload,
                              description=op.get("description") or op.get("summary")))
    else:  # AsyncAPI 2.x
        for cname, cdef in (doc.get("channels") or {}).items():
            for action, op_key in (("send", "publish"), ("receive", "subscribe")):
                op = (cdef or {}).get(op_key)
                if not op:
                    continue
                msgs = op.get("messages") or []
                inline = op.get("message")
                payload, mname = None, None
                if msgs:
                    payload, mname = _message_payload(ctx, path, models, msgs[0].get("$ref"), msgs[0])
                elif inline:
                    payload, mname = _message_payload(ctx, path, models, None, inline)
                op_id = op.get("operationId") or snake(action + " " + cname + " " + (mname or ""))
                flows.append(Flow(name=op_id, action=action, channel=cname, topic=cname,
                                  message_name=mname, payload=payload,
                                  description=op.get("summary") or op.get("description")))
    return doc, flows


# --------------------------------------------------------------------------
# entry point
# --------------------------------------------------------------------------

def load_all(openapi_files, asyncapi_files, api_name=None):
    ctx = SpecContext()
    models = {}
    endpoints, flows, sources = [], [], []
    first_doc = None
    for path in openapi_files or []:
        doc, eps = load_openapi(ctx, path, models)
        if first_doc is None:
            first_doc = doc
        endpoints.extend(eps)
        sources.append(path)
    for path in asyncapi_files or []:
        doc, fls = load_asyncapi(ctx, path, models)
        if first_doc is None:
            first_doc = doc
        flows.extend(fls)
        sources.append(path)
    info = (first_doc or {}).get("info") or {}
    name = api_name or info.get("title") or "Generated API"
    version = str(info.get("version") or "1.0.0")
    endpoints.sort(key=lambda e: (e.tag, e.path, e.method))
    flows.sort(key=lambda f: (f.channel, f.action, f.name))
    return ApiIR(api_name=name, version=version, models=models,
                 endpoints=endpoints, flows=flows, sources=sources)
