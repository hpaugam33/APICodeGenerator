import os
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from apigen.loader import load_all
from apigen.naming import camel, kebab, pascal, screaming, snake
from apigen.ir import TypeRef
from apigen.emit.quarkus import java_type, emit_quarkus
from apigen.emit.rust import rust_type, emit_rust

SPECS = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "specs")
OPENAPI = os.path.join(SPECS, "order-service.openapi.yaml")
ASYNCAPI = os.path.join(SPECS, "order-events.asyncapi.yaml")


class TestNaming(unittest.TestCase):
    def test_accents_and_spaces(self):
        self.assertEqual(pascal("Détails de la commande"), "DetailsDeLaCommande")
        self.assertEqual(snake("Détails de la commande"), "details_de_la_commande")
        self.assertEqual(camel("orderId"), "orderId")
        self.assertEqual(snake("orderId"), "order_id")
        self.assertEqual(pascal("OrderStatus"), "OrderStatus")
        self.assertEqual(screaming("order status"), "ORDER_STATUS")
        self.assertEqual(kebab("Order Service API"), "order-service-api")


class TestTypeMapping(unittest.TestCase):
    def test_java(self):
        imp = set()
        self.assertEqual(java_type(TypeRef("string", format="uuid"), imp), "UUID")
        self.assertIn("java.util.UUID", imp)
        self.assertEqual(java_type(TypeRef("string", format="date-time"), set()), "OffsetDateTime")
        self.assertEqual(java_type(TypeRef("integer", format="int32"), set()), "Integer")
        self.assertEqual(java_type(TypeRef("integer", format="int64"), set()), "Long")
        self.assertEqual(java_type(TypeRef("number", format="double"), set()), "Double")
        self.assertEqual(java_type(TypeRef("boolean"), set()), "Boolean")
        self.assertEqual(java_type(TypeRef("array", item=TypeRef("ref", ref="Order")), set()),
                         "List<Order>")
        self.assertEqual(java_type(TypeRef("ref", ref="Customer"), set()), "Customer")

    def test_rust(self):
        imp = set()
        self.assertEqual(rust_type(TypeRef("string", format="uuid"), imp), "Uuid")
        self.assertIn("uuid::Uuid", imp)
        self.assertEqual(rust_type(TypeRef("string", format="date-time"), set()), "DateTime<Utc>")
        self.assertEqual(rust_type(TypeRef("integer", format="int32"), set()), "i32")
        self.assertEqual(rust_type(TypeRef("integer", format="int64"), set()), "i64")
        self.assertEqual(rust_type(TypeRef("number", format="float"), set()), "f32")
        self.assertEqual(rust_type(TypeRef("boolean"), set()), "bool")
        self.assertEqual(rust_type(TypeRef("array", item=TypeRef("ref", ref="OrderLine")), set()),
                         "Vec<OrderLine>")
        self.assertEqual(rust_type(TypeRef("ref", ref="Customer"), set()), "Customer")


class TestLoad(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ir = load_all([OPENAPI], [ASYNCAPI])

    def test_models(self):
        for name in ("Order", "OrderStatus", "OrderLine", "Customer", "Address",
                     "CreateOrderRequest", "OrderLineInput", "UpdateOrderStatusRequest",
                     "CreateCustomerRequest", "Error", "OrderEvent", "PaymentEvent"):
            self.assertIn(name, self.ir.models, name)
        order = self.ir.models["Order"]
        self.assertEqual(order.kind, "object")
        req = {f.name for f in order.fields if f.required}
        self.assertEqual(req, {"id", "customerId", "status", "lines", "createdAt"})
        status = self.ir.models["OrderStatus"]
        self.assertEqual(status.kind, "enum")
        self.assertEqual(status.enum_values, ["PENDING", "CONFIRMED", "SHIPPED", "CANCELLED"])

    def test_cross_file_ref(self):
        # OrderEvent.order references ./order-service.openapi.yaml#/components/schemas/Order
        order_event = self.ir.models["OrderEvent"]
        order_field = next(f for f in order_event.fields if f.name == "order")
        self.assertTrue(order_field.type.is_ref())
        self.assertEqual(order_field.type.ref, "Order")

    def test_endpoints(self):
        self.assertEqual(len(self.ir.endpoints), 7)
        by_id = {e.operation_id: e for e in self.ir.endpoints}
        self.assertEqual(by_id["listOrders"].method, "GET")
        self.assertEqual(by_id["createOrder"].request_body.ref, "CreateOrderRequest")
        self.assertEqual(by_id["createOrder"].response_status, "201")
        self.assertEqual(by_id["getOrder"].error_response.ref, "Error")
        self.assertIsNone(by_id["deleteOrder"].response)
        self.assertEqual(by_id["deleteOrder"].response_status, "204")
        self.assertEqual({p.name for p in by_id["getOrder"].params}, {"orderId"})
        self.assertEqual({p.name for p in by_id["listOrders"].params}, {"status", "limit", "offset"})

    def test_flows(self):
        self.assertEqual(len(self.ir.flows), 4)
        sends = [f for f in self.ir.flows if f.action == "send"]
        receives = [f for f in self.ir.flows if f.action == "receive"]
        self.assertEqual(len(sends), 2)
        self.assertEqual(len(receives), 2)
        self.assertEqual({f.channel for f in sends}, {"order.events"})
        self.assertEqual({f.channel for f in receives}, {"payment.events"})
        topic = {f.channel: f.topic for f in self.ir.flows}
        self.assertEqual(topic["order.events"], "orders")
        self.assertEqual(topic["payment.events"], "payments")
        self.assertEqual(sends[0].payload.ref, "OrderEvent")

    def test_asyncapi_2x(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = os.path.join(tmp, "legacy.asyncapi.yaml")
            with open(p, "w") as fh:
                fh.write("""asyncapi: '2.6.0'
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
""")
            ir = load_all([], [p])
            self.assertEqual(len(ir.flows), 1)
            self.assertEqual(ir.flows[0].action, "send")
            self.assertEqual(ir.flows[0].channel, "order/created")
            self.assertEqual(ir.flows[0].payload.ref, "MessagePayload")
            self.assertIn("MessagePayload", ir.models)


class TestEmitQuarkus(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ir = load_all([OPENAPI], [ASYNCAPI])
        cls.tmp = tempfile.mkdtemp()
        cls.files = emit_quarkus(cls.ir, cls.tmp, package="com.example.orderservice",
                                 group_id="com.example.orderservice", artifact="order-service",
                                 command="test")

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def read(self, rel):
        with open(os.path.join(self.tmp, rel)) as fh:
            return fh.read()

    def test_files(self):
        rels = {os.path.relpath(p, self.tmp) for p in self.files}
        self.assertIn("pom.xml", rels)
        self.assertIn("src/main/resources/application.properties", rels)
        self.assertIn("src/main/java/com/example/orderservice/model/Order.java", rels)
        self.assertIn("src/main/java/com/example/orderservice/model/OrderStatus.java", rels)
        self.assertIn("src/main/java/com/example/orderservice/api/OrdersResource.java", rels)
        self.assertIn("src/main/java/com/example/orderservice/api/CustomersResource.java", rels)
        self.assertIn("src/main/java/com/example/orderservice/messaging/OrderEventsProducer.java", rels)
        self.assertIn("src/main/java/com/example/orderservice/messaging/PaymentEventsConsumer.java", rels)

    def test_model_record(self):
        src = self.read("src/main/java/com/example/orderservice/model/Order.java")
        self.assertIn("public record Order(", src)
        self.assertIn("@NotNull UUID id", src)
        self.assertIn("List<OrderLine> lines", src)
        self.assertIn("OffsetDateTime createdAt", src)
        enum_src = self.read("src/main/java/com/example/orderservice/model/OrderStatus.java")
        self.assertIn("public enum OrderStatus {", enum_src)
        self.assertIn("PENDING, CONFIRMED, SHIPPED, CANCELLED", enum_src)

    def test_resource(self):
        src = self.read("src/main/java/com/example/orderservice/api/OrdersResource.java")
        self.assertIn('@Path("/orders")', src)
        self.assertIn("public class OrdersResource", src)
        self.assertIn("public List<Order> listOrders(", src)
        self.assertIn('@QueryParam("status") OrderStatus status', src)
        self.assertIn("public Order getOrder(@PathParam(\"orderId\") UUID orderId)", src)
        self.assertIn('public Response deleteOrder(@PathParam("orderId") UUID orderId)', src)
        self.assertIn("public Order createOrder(CreateOrderRequest body)", src)
        self.assertIn('throw new UnsupportedOperationException("TODO implement getOrder");', src)
        self.assertIn("import com.example.orderservice.model.Order;", src)
        self.assertIn("import com.example.orderservice.model.OrderStatus;", src)

    def test_messaging(self):
        prod = self.read("src/main/java/com/example/orderservice/messaging/OrderEventsProducer.java")
        self.assertIn('@Channel("order.events")', prod)
        self.assertIn("public CompletionStage<Void> publishOrderCreated(OrderEvent event)", prod)
        cons = self.read("src/main/java/com/example/orderservice/messaging/PaymentEventsConsumer.java")
        self.assertIn('@Incoming("payment.events")', cons)
        self.assertIn("public CompletionStage<Void> consumePaymentConfirmed(PaymentEvent event)", cons)
        self.assertIn("protected CompletionStage<Void> handleConsumePaymentConfirmed(PaymentEvent event)", cons)
        self.assertIn("protected CompletionStage<Void> handleConsumePaymentRejected(PaymentEvent event)", cons)
        self.assertIn("import com.example.orderservice.model.PaymentEvent;", cons)
        props = self.read("src/main/resources/application.properties")
        self.assertIn("mp.messaging.outgoing.order.events.topic=orders", props)
        self.assertIn("mp.messaging.incoming.payment.events.topic=payments", props)

    def test_pom(self):
        pom = self.read("pom.xml")
        self.assertIn("<artifactId>order-service</artifactId>", pom)
        self.assertIn("quarkus-rest-jackson", pom)
        self.assertIn("quarkus-smallrye-reactive-messaging-kafka", pom)


class TestEmitRust(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ir = load_all([OPENAPI], [ASYNCAPI])
        cls.tmp = tempfile.mkdtemp()
        cls.files = emit_rust(cls.ir, cls.tmp, crate_name="order_service", command="test")

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def read(self, rel):
        with open(os.path.join(self.tmp, rel)) as fh:
            return fh.read()

    def test_files(self):
        rels = {os.path.relpath(p, self.tmp) for p in self.files}
        self.assertIn("Cargo.toml", rels)
        self.assertIn("src/main.rs", rels)
        self.assertIn("src/api/mod.rs", rels)
        self.assertIn("src/api/orders.rs", rels)
        self.assertIn("src/model/order.rs", rels)
        self.assertIn("src/messaging/mod.rs", rels)

    def test_model(self):
        src = self.read("src/model/order.rs")
        self.assertIn("pub struct Order {", src)
        self.assertIn("pub id: Uuid,", src)
        self.assertIn("pub lines: Vec<OrderLine>,", src)
        self.assertIn('skip_serializing_if = "Option::is_none"', src)
        self.assertIn("pub total_amount: Option<f64>,", src)
        enum_src = self.read("src/model/order_status.rs")
        self.assertIn("pub enum OrderStatus {", enum_src)
        self.assertIn("Pending,", enum_src)
        self.assertIn('rename_all = "SCREAMING_SNAKE_CASE"', enum_src)

    def test_api(self):
        src = self.read("src/api/orders.rs")
        self.assertIn("pub async fn list_orders(", src)
        self.assertIn("Query(query_params): Query<ListOrdersQuery>", src)
        self.assertIn("pub struct ListOrdersQuery {", src)
        self.assertIn("pub status: Option<OrderStatus>,", src)
        self.assertIn("Path(path_params): Path<GetOrderPath>", src)
        self.assertIn('#[serde(rename = "orderId")]', src)
        self.assertIn("Json(body): Json<CreateOrderRequest>", src)
        self.assertIn("Result<Json<Vec<Order>>, ApiError>", src)
        self.assertIn("Result<StatusCode, ApiError>", src)
        self.assertIn('.route("/orders", routing::get(list_orders).post(create_order))', src)
        self.assertIn('.route("/orders/{orderId}", routing::get(get_order).delete(delete_order))', src)

    def test_main_and_messaging(self):
        main = self.read("src/main.rs")
        self.assertIn(".merge(api::orders::router())", main)
        self.assertIn(".merge(api::customers::router())", main)
        msg = self.read("src/messaging/mod.rs")
        self.assertIn("pub trait OrderEventsProducer {", msg)
        self.assertIn("async fn publish_order_created(&self, event: OrderEvent) -> anyhow::Result<()>;", msg)
        self.assertIn("pub trait PaymentEventsConsumer {", msg)
        self.assertIn("async fn consume_payment_confirmed(&self, event: PaymentEvent) -> anyhow::Result<()>;", msg)

    def test_deterministic(self):
        tmp2 = tempfile.mkdtemp()
        try:
            files2 = emit_rust(self.ir, tmp2, crate_name="order_service", command="test")
            for p in self.files:
                rel = os.path.relpath(p, self.tmp)
                with open(p) as a, open(os.path.join(tmp2, rel)) as b:
                    self.assertEqual(a.read(), b.read(), rel)
        finally:
            shutil.rmtree(tmp2, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
