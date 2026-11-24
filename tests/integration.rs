//! Port of tests/test_apigen.py (Python unittest suite).

use std::collections::BTreeSet;
use std::path::PathBuf;

use apigen::emit::quarkus::{emit_quarkus, java_type};
use apigen::emit::rust::{emit_rust, rust_type};
use apigen::ir::TypeRef;
use apigen::loader::load_all;
use apigen::naming::{camel, kebab, pascal, screaming, snake};

fn specs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("specs")
}

fn openapi() -> PathBuf {
    specs_dir().join("order-service.openapi.yaml")
}

fn asyncapi() -> PathBuf {
    specs_dir().join("order-events.asyncapi.yaml")
}

fn load_test_ir() -> apigen::ir::ApiIR {
    load_all(&[openapi()], &[asyncapi()], None).unwrap()
}

// ---------------------------------------------------------------------------
// naming
// ---------------------------------------------------------------------------

#[test]
fn test_accents_and_spaces() {
    assert_eq!(pascal("Détails de la commande"), "DetailsDeLaCommande");
    assert_eq!(snake("Détails de la commande"), "details_de_la_commande");
    assert_eq!(camel("orderId"), "orderId");
    assert_eq!(snake("orderId"), "order_id");
    assert_eq!(pascal("OrderStatus"), "OrderStatus");
    assert_eq!(screaming("order status"), "ORDER_STATUS");
    assert_eq!(kebab("Order Service API"), "order-service-api");
}

// ---------------------------------------------------------------------------
// type mapping
// ---------------------------------------------------------------------------

#[test]
fn test_java_types() {
    let mut imp = BTreeSet::new();
    assert_eq!(java_type(&TypeRef::with_format("string", Some("uuid".into())), &mut imp), "UUID");
    assert!(imp.contains("java.util.UUID"));
    assert_eq!(java_type(&TypeRef::with_format("string", Some("date-time".into())), &mut BTreeSet::new()), "OffsetDateTime");
    assert_eq!(java_type(&TypeRef::with_format("integer", Some("int32".into())), &mut BTreeSet::new()), "Integer");
    assert_eq!(java_type(&TypeRef::with_format("integer", Some("int64".into())), &mut BTreeSet::new()), "Long");
    assert_eq!(java_type(&TypeRef::with_format("number", Some("double".into())), &mut BTreeSet::new()), "Double");
    assert_eq!(java_type(&TypeRef::new("boolean"), &mut BTreeSet::new()), "Boolean");
    assert_eq!(
        java_type(&TypeRef::array(TypeRef::ref_("Order")), &mut BTreeSet::new()),
        "List<Order>"
    );
    assert_eq!(java_type(&TypeRef::ref_("Customer"), &mut BTreeSet::new()), "Customer");
}

#[test]
fn test_rust_types() {
    let mut imp = BTreeSet::new();
    assert_eq!(rust_type(&TypeRef::with_format("string", Some("uuid".into())), &mut imp), "Uuid");
    assert!(imp.contains("uuid::Uuid"));
    assert_eq!(
        rust_type(&TypeRef::with_format("string", Some("date-time".into())), &mut BTreeSet::new()),
        "DateTime<Utc>"
    );
    assert_eq!(rust_type(&TypeRef::with_format("integer", Some("int32".into())), &mut BTreeSet::new()), "i32");
    assert_eq!(rust_type(&TypeRef::with_format("integer", Some("int64".into())), &mut BTreeSet::new()), "i64");
    assert_eq!(rust_type(&TypeRef::with_format("number", Some("float".into())), &mut BTreeSet::new()), "f32");
    assert_eq!(rust_type(&TypeRef::new("boolean"), &mut BTreeSet::new()), "bool");
    assert_eq!(
        rust_type(&TypeRef::array(TypeRef::ref_("OrderLine")), &mut BTreeSet::new()),
        "Vec<OrderLine>"
    );
    assert_eq!(rust_type(&TypeRef::ref_("Customer"), &mut BTreeSet::new()), "Customer");
}

// ---------------------------------------------------------------------------
// loader
// ---------------------------------------------------------------------------

#[test]
fn test_models() {
    let ir = load_test_ir();
    for name in [
        "Order", "OrderStatus", "OrderLine", "Customer", "Address", "CreateOrderRequest",
        "OrderLineInput", "UpdateOrderStatusRequest", "CreateCustomerRequest", "Error",
        "OrderEvent", "PaymentEvent",
    ] {
        assert!(ir.models.contains_key(name), "missing model {name}");
    }
    let order = &ir.models["Order"];
    assert_eq!(order.kind, "object");
    let req: BTreeSet<&str> = order.fields.iter().filter(|f| f.required).map(|f| f.name.as_str()).collect();
    assert_eq!(req, ["id", "customerId", "status", "lines", "createdAt"].into_iter().collect());
    let status = &ir.models["OrderStatus"];
    assert_eq!(status.kind, "enum");
    assert_eq!(status.enum_values, ["PENDING", "CONFIRMED", "SHIPPED", "CANCELLED"]);
}

#[test]
fn test_cross_file_ref() {
    // OrderEvent.order references ./order-service.openapi.yaml#/components/schemas/Order
    let ir = load_test_ir();
    let order_event = &ir.models["OrderEvent"];
    let order_field = order_event.fields.iter().find(|f| f.name == "order").unwrap();
    assert!(order_field.r#type.is_ref());
    assert_eq!(order_field.r#type.ref_name.as_deref(), Some("Order"));
}

#[test]
fn test_endpoints() {
    let ir = load_test_ir();
    assert_eq!(ir.endpoints.len(), 7);
    let by_id = |id: &str| ir.endpoints.iter().find(|e| e.operation_id == id).unwrap();
    assert_eq!(by_id("listOrders").method, "GET");
    assert_eq!(by_id("createOrder").request_body.as_ref().unwrap().ref_name.as_deref(), Some("CreateOrderRequest"));
    assert_eq!(by_id("createOrder").response_status, "201");
    assert_eq!(by_id("getOrder").error_response.as_ref().unwrap().ref_name.as_deref(), Some("Error"));
    assert!(by_id("deleteOrder").response.is_none());
    assert_eq!(by_id("deleteOrder").response_status, "204");
    let get_params: BTreeSet<&str> = by_id("getOrder").params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(get_params, ["orderId"].into_iter().collect());
    let list_params: BTreeSet<&str> =
        by_id("listOrders").params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(list_params, ["status", "limit", "offset"].into_iter().collect());
}

#[test]
fn test_flows() {
    let ir = load_test_ir();
    assert_eq!(ir.flows.len(), 4);
    let sends: Vec<_> = ir.flows.iter().filter(|f| f.action == "send").collect();
    let receives: Vec<_> = ir.flows.iter().filter(|f| f.action == "receive").collect();
    assert_eq!(sends.len(), 2);
    assert_eq!(receives.len(), 2);
    let send_channels: BTreeSet<&str> = sends.iter().map(|f| f.channel.as_str()).collect();
    assert_eq!(send_channels, ["order.events"].into_iter().collect());
    let recv_channels: BTreeSet<&str> = receives.iter().map(|f| f.channel.as_str()).collect();
    assert_eq!(recv_channels, ["payment.events"].into_iter().collect());
    let topic = |ch: &str| ir.flows.iter().find(|f| f.channel == ch).unwrap().topic.as_deref();
    assert_eq!(topic("order.events"), Some("orders"));
    assert_eq!(topic("payment.events"), Some("payments"));
    assert_eq!(sends[0].payload.as_ref().unwrap().ref_name.as_deref(), Some("OrderEvent"));
}

#[test]
fn test_asyncapi_2x() {
    let dir = std::env::temp_dir().join(format!("apigen-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("legacy.asyncapi.yaml");
    std::fs::write(
        &p,
        r#"asyncapi: '2.6.0'
info: {title: Legacy, version: '1.0'}
channels:
  order/created:
    publish:
      operationId: publishOrderCreated
      message:
        payload:
          type: object
          required: [orderId]
          properties:
            orderId: {type: string, format: uuid}
"#,
    )
    .unwrap();
    let ir = load_all(&[], &[p.clone()], None).unwrap();
    assert_eq!(ir.flows.len(), 1);
    assert_eq!(ir.flows[0].action, "send");
    assert_eq!(ir.flows[0].channel, "order/created");
    assert_eq!(ir.flows[0].payload.as_ref().unwrap().ref_name.as_deref(), Some("MessagePayload"));
    assert!(ir.models.contains_key("MessagePayload"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// emit quarkus
// ---------------------------------------------------------------------------

fn emit_quarkus_tmp(ir: &apigen::ir::ApiIR) -> (PathBuf, Vec<PathBuf>) {
    let tmp = std::env::temp_dir().join(format!("apigen-q-{}-{}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&tmp).unwrap();
    let files = emit_quarkus(
        ir,
        &tmp,
        "com.example.orderservice",
        "com.example.orderservice",
        Some("order-service"),
        "test",
    )
    .unwrap();
    (tmp, files)
}

fn emit_rust_tmp(ir: &apigen::ir::ApiIR) -> (PathBuf, Vec<PathBuf>) {
    let tmp = std::env::temp_dir().join(format!("apigen-r-{}-{}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&tmp).unwrap();
    let files = emit_rust(ir, &tmp, Some("order_service"), "test").unwrap();
    (tmp, files)
}

fn rand_suffix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
}

fn read(tmp: &PathBuf, rel: &str) -> String {
    std::fs::read_to_string(tmp.join(rel)).unwrap()
}

#[test]
fn test_quarkus_files() {
    let ir = load_test_ir();
    let (tmp, files) = emit_quarkus_tmp(&ir);
    let rels: BTreeSet<String> = files
        .iter()
        .map(|p| p.strip_prefix(&tmp).unwrap().to_string_lossy().replace('\\', "/"))
        .collect();
    for expected in [
        "pom.xml",
        "src/main/resources/application.properties",
        "src/main/java/com/example/orderservice/model/Order.java",
        "src/main/java/com/example/orderservice/model/OrderStatus.java",
        "src/main/java/com/example/orderservice/api/OrdersResource.java",
        "src/main/java/com/example/orderservice/api/CustomersResource.java",
        "src/main/java/com/example/orderservice/messaging/OrderEventsProducer.java",
        "src/main/java/com/example/orderservice/messaging/PaymentEventsConsumer.java",
    ] {
        assert!(rels.contains(expected), "missing {expected}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_quarkus_model_record() {
    let ir = load_test_ir();
    let (tmp, _) = emit_quarkus_tmp(&ir);
    let src = read(&tmp, "src/main/java/com/example/orderservice/model/Order.java");
    assert!(src.contains("public record Order("), "{src}");
    assert!(src.contains("@NotNull UUID id"), "{src}");
    assert!(src.contains("List<OrderLine> lines"), "{src}");
    assert!(src.contains("OffsetDateTime createdAt"), "{src}");
    let enum_src = read(&tmp, "src/main/java/com/example/orderservice/model/OrderStatus.java");
    assert!(enum_src.contains("public enum OrderStatus {"), "{enum_src}");
    assert!(enum_src.contains("PENDING, CONFIRMED, SHIPPED, CANCELLED"), "{enum_src}");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_quarkus_resource() {
    let ir = load_test_ir();
    let (tmp, _) = emit_quarkus_tmp(&ir);
    let src = read(&tmp, "src/main/java/com/example/orderservice/api/OrdersResource.java");
    for expected in [
        "@Path(\"/orders\")",
        "public class OrdersResource",
        "public List<Order> listOrders(",
        "@QueryParam(\"status\") OrderStatus status",
        "public Order getOrder(@PathParam(\"orderId\") UUID orderId)",
        "public Response deleteOrder(@PathParam(\"orderId\") UUID orderId)",
        "public Order createOrder(CreateOrderRequest body)",
        "throw new UnsupportedOperationException(\"TODO implement getOrder\");",
        "import com.example.orderservice.model.Order;",
        "import com.example.orderservice.model.OrderStatus;",
    ] {
        assert!(src.contains(expected), "missing {expected} in:\n{src}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_quarkus_messaging() {
    let ir = load_test_ir();
    let (tmp, _) = emit_quarkus_tmp(&ir);
    let prod = read(&tmp, "src/main/java/com/example/orderservice/messaging/OrderEventsProducer.java");
    assert!(prod.contains("@Channel(\"order.events\")"), "{prod}");
    assert!(prod.contains("public CompletionStage<Void> publishOrderCreated(OrderEvent event)"), "{prod}");
    let cons = read(&tmp, "src/main/java/com/example/orderservice/messaging/PaymentEventsConsumer.java");
    assert!(cons.contains("@Incoming(\"payment.events\")"), "{cons}");
    assert!(cons.contains("public CompletionStage<Void> consumePaymentConfirmed(PaymentEvent event)"), "{cons}");
    assert!(cons.contains("protected CompletionStage<Void> handleConsumePaymentConfirmed(PaymentEvent event)"), "{cons}");
    assert!(cons.contains("protected CompletionStage<Void> handleConsumePaymentRejected(PaymentEvent event)"), "{cons}");
    assert!(cons.contains("import com.example.orderservice.model.PaymentEvent;"), "{cons}");
    let props = read(&tmp, "src/main/resources/application.properties");
    assert!(props.contains("mp.messaging.outgoing.order.events.topic=orders"), "{props}");
    assert!(props.contains("mp.messaging.incoming.payment.events.topic=payments"), "{props}");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_quarkus_pom() {
    let ir = load_test_ir();
    let (tmp, _) = emit_quarkus_tmp(&ir);
    let pom = read(&tmp, "pom.xml");
    assert!(pom.contains("<artifactId>order-service</artifactId>"), "{pom}");
    assert!(pom.contains("quarkus-rest-jackson"), "{pom}");
    assert!(pom.contains("quarkus-smallrye-reactive-messaging-kafka"), "{pom}");
    let _ = std::fs::remove_dir_all(&tmp);
}

// ---------------------------------------------------------------------------
// emit rust
// ---------------------------------------------------------------------------

#[test]
fn test_rust_files() {
    let ir = load_test_ir();
    let (tmp, files) = emit_rust_tmp(&ir);
    let rels: BTreeSet<String> = files
        .iter()
        .map(|p| p.strip_prefix(&tmp).unwrap().to_string_lossy().replace('\\', "/"))
        .collect();
    for expected in [
        "Cargo.toml",
        "src/main.rs",
        "src/api/mod.rs",
        "src/api/orders.rs",
        "src/model/order.rs",
        "src/messaging/mod.rs",
    ] {
        assert!(rels.contains(expected), "missing {expected}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_rust_model() {
    let ir = load_test_ir();
    let (tmp, _) = emit_rust_tmp(&ir);
    let src = read(&tmp, "src/model/order.rs");
    for expected in [
        "pub struct Order {",
        "pub id: Uuid,",
        "pub lines: Vec<OrderLine>,",
        "skip_serializing_if = \"Option::is_none\"",
        "pub total_amount: Option<f64>,",
    ] {
        assert!(src.contains(expected), "missing {expected} in:\n{src}");
    }
    let enum_src = read(&tmp, "src/model/order_status.rs");
    assert!(enum_src.contains("pub enum OrderStatus {"), "{enum_src}");
    assert!(enum_src.contains("Pending,"), "{enum_src}");
    assert!(enum_src.contains("rename_all = \"SCREAMING_SNAKE_CASE\""), "{enum_src}");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_rust_api() {
    let ir = load_test_ir();
    let (tmp, _) = emit_rust_tmp(&ir);
    let src = read(&tmp, "src/api/orders.rs");
    for expected in [
        "pub async fn list_orders(",
        "Query(query_params): Query<ListOrdersQuery>",
        "pub struct ListOrdersQuery {",
        "pub status: Option<OrderStatus>,",
        "Path(path_params): Path<GetOrderPath>",
        "#[serde(rename = \"orderId\")]",
        "Json(body): Json<CreateOrderRequest>",
        "Result<Json<Vec<Order>>, ApiError>",
        "Result<StatusCode, ApiError>",
        ".route(\"/orders\", routing::get(list_orders).post(create_order))",
        ".route(\"/orders/{orderId}\", routing::get(get_order).delete(delete_order))",
    ] {
        assert!(src.contains(expected), "missing {expected} in:\n{src}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_rust_main_and_messaging() {
    let ir = load_test_ir();
    let (tmp, _) = emit_rust_tmp(&ir);
    let main = read(&tmp, "src/main.rs");
    assert!(main.contains(".merge(api::orders::router())"), "{main}");
    assert!(main.contains(".merge(api::customers::router())"), "{main}");
    let msg = read(&tmp, "src/messaging/mod.rs");
    assert!(msg.contains("pub trait OrderEventsProducer {"), "{msg}");
    assert!(
        msg.contains("async fn publish_order_created(&self, event: OrderEvent) -> anyhow::Result<()>;"),
        "{msg}"
    );
    assert!(msg.contains("pub trait PaymentEventsConsumer {"), "{msg}");
    assert!(
        msg.contains("async fn consume_payment_confirmed(&self, event: PaymentEvent) -> anyhow::Result<()>;"),
        "{msg}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_rust_deterministic() {
    let ir = load_test_ir();
    let (tmp1, files1) = emit_rust_tmp(&ir);
    let (tmp2, files2) = emit_rust_tmp(&ir);
    assert_eq!(files1.len(), files2.len());
    for p in &files1 {
        let rel = p.strip_prefix(&tmp1).unwrap();
        let a = std::fs::read_to_string(p).unwrap();
        let b = std::fs::read_to_string(tmp2.join(rel)).unwrap();
        assert_eq!(a, b, "mismatch in {}", rel.display());
    }
    let _ = std::fs::remove_dir_all(&tmp1);
    let _ = std::fs::remove_dir_all(&tmp2);
}
