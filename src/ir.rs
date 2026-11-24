//! Intermediate representation shared by all language emitters.

use std::collections::BTreeMap;

use yaml_rust2::Yaml;

#[derive(Debug, Clone, PartialEq)]
pub struct TypeRef {
    pub kind: String,                // string|integer|number|boolean|array|map|object|any|ref
    pub format: Option<String>,      // date, date-time, uuid, email, byte, binary, int32, int64, float, double ...
    pub item: Option<Box<TypeRef>>,  // array element type
    pub value: Option<Box<TypeRef>>, // map value type
    pub ref_name: Option<String>,    // referenced model name when kind == "ref"
}

impl TypeRef {
    pub fn new(kind: &str) -> Self {
        TypeRef { kind: kind.to_string(), format: None, item: None, value: None, ref_name: None }
    }

    pub fn any() -> Self {
        TypeRef::new("any")
    }

    pub fn with_format(kind: &str, format: Option<String>) -> Self {
        TypeRef { format, ..TypeRef::new(kind) }
    }

    pub fn ref_(name: &str) -> Self {
        TypeRef { ref_name: Some(name.to_string()), ..TypeRef::new("ref") }
    }

    pub fn array(item: TypeRef) -> Self {
        TypeRef { item: Some(Box::new(item)), ..TypeRef::new("array") }
    }

    pub fn map(value: TypeRef) -> Self {
        TypeRef { value: Some(Box::new(value)), ..TypeRef::new("map") }
    }

    pub fn is_ref(&self) -> bool {
        self.kind == "ref"
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,                    // original property name in the spec
    pub r#type: TypeRef,
    pub required: bool,
    pub description: Option<String>,
    pub constraints: BTreeMap<String, Yaml>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    pub kind: String,                    // object | enum
    pub description: Option<String>,
    pub fields: Vec<Field>,              // object models
    pub enum_values: Vec<String>,        // enum models
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub location: String,                // path | query | header
    pub r#type: TypeRef,
    pub required: bool,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Endpoint {
    pub method: String,                  // GET, POST, ...
    pub path: String,                    // /orders/{orderId}
    pub operation_id: String,
    pub tag: String,
    pub summary: Option<String>,
    pub params: Vec<Param>,
    pub request_body: Option<TypeRef>,
    pub response: Option<TypeRef>,
    pub response_status: String,
    pub error_response: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Flow {
    /// One AsyncAPI operation: the service sends or receives a message on a channel.
    pub name: String,                    // operation id, e.g. publishOrderCreated
    pub action: String,                  // send | receive
    pub channel: String,                 // channel address, e.g. order.events
    pub topic: Option<String>,           // broker topic from bindings, if any
    pub message_name: Option<String>,
    pub payload: Option<TypeRef>,
    pub description: Option<String>,
}

/// Flows grouped by channel, as used by the messaging emitters.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChannelFlows {
    pub topic: Option<String>,
    pub send: Vec<Flow>,
    pub receive: Vec<Flow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiIR {
    pub api_name: String,
    pub version: String,
    pub models: BTreeMap<String, Model>, // name -> Model
    pub endpoints: Vec<Endpoint>,
    pub flows: Vec<Flow>,
    pub sources: Vec<String>,
}

impl ApiIR {
    pub fn tags(&self) -> Vec<String> {
        let mut seen = Vec::new();
        for ep in &self.endpoints {
            if !seen.contains(&ep.tag) {
                seen.push(ep.tag.clone());
            }
        }
        seen
    }

    pub fn endpoints_for_tag(&self, tag: &str) -> Vec<&Endpoint> {
        self.endpoints.iter().filter(|ep| ep.tag == tag).collect()
    }

    pub fn flows_by_channel(&self) -> BTreeMap<String, ChannelFlows> {
        let mut out: BTreeMap<String, ChannelFlows> = BTreeMap::new();
        for f in &self.flows {
            let entry = out.entry(f.channel.clone()).or_insert_with(|| ChannelFlows {
                topic: f.topic.clone(),
                send: vec![],
                receive: vec![],
            });
            match f.action.as_str() {
                "send" => entry.send.push(f.clone()),
                "receive" => entry.receive.push(f.clone()),
                _ => entry.receive.push(f.clone()),
            }
        }
        out
    }
}
