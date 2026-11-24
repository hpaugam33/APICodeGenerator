"""Intermediate representation shared by all language emitters."""

from dataclasses import dataclass, field


@dataclass
class TypeRef:
    kind: str                       # string|integer|number|boolean|array|map|object|any|ref
    format: str = None              # date, date-time, uuid, email, byte, binary, int32, int64, float, double ...
    item: "TypeRef" = None          # array element type
    value: "TypeRef" = None         # map value type
    ref: str = None                 # referenced model name when kind == 'ref'

    def is_ref(self) -> bool:
        return self.kind == "ref"


@dataclass
class Field:
    name: str                       # original property name in the spec
    type: TypeRef
    required: bool = False
    description: str = None
    constraints: dict = field(default_factory=dict)


@dataclass
class Model:
    name: str
    kind: str                       # object | enum
    description: str = None
    fields: list = field(default_factory=list)        # object models
    enum_values: list = field(default_factory=list)   # enum models


@dataclass
class Param:
    name: str
    location: str                   # path | query | header
    type: TypeRef
    required: bool = False
    description: str = None


@dataclass
class Endpoint:
    method: str                     # GET, POST, ...
    path: str                       # /orders/{orderId}
    operation_id: str
    tag: str
    summary: str = None
    params: list = field(default_factory=list)        # Param
    request_body: TypeRef = None
    response: TypeRef = None
    response_status: str = "200"
    error_response: TypeRef = None


@dataclass
class Flow:
    """One AsyncAPI operation: the service sends or receives a message on a channel."""
    name: str                       # operation id, e.g. publishOrderCreated
    action: str                     # send | receive
    channel: str                    # channel address, e.g. order.events
    topic: str = None               # broker topic from bindings, if any
    message_name: str = None
    payload: TypeRef = None
    description: str = None


@dataclass
class ApiIR:
    api_name: str
    version: str
    models: dict                    # name -> Model
    endpoints: list                 # Endpoint
    flows: list                     # Flow
    sources: list = field(default_factory=list)

    def tags(self) -> list:
        seen, out = set(), []
        for ep in self.endpoints:
            if ep.tag not in seen:
                seen.add(ep.tag)
                out.append(ep.tag)
        return out

    def endpoints_for_tag(self, tag: str) -> list:
        return [ep for ep in self.endpoints if ep.tag == tag]

    def flows_by_channel(self) -> dict:
        out = {}
        for f in self.flows:
            out.setdefault(f.channel, {"topic": f.topic, "send": [], "receive": []})
            out[f.channel][f.action].append(f)
        return out
