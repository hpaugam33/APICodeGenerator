// GENERATED CODE — DO NOT EDIT.
// Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//   python -m apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

package com.example.orderservice.messaging;

import com.example.orderservice.model.OrderEvent;
import jakarta.enterprise.context.ApplicationScoped;
import java.util.concurrent.CompletionStage;
import org.eclipse.microprofile.reactive.messaging.Channel;
import org.eclipse.microprofile.reactive.messaging.Emitter;

/** Publishes on channel `order.events` (topic: orders). */
@ApplicationScoped
public class OrderEventsProducer {

    @Channel("order.events")
    Emitter<OrderEvent> orderEventEmitter;

    public CompletionStage<Void> publishOrderCreated(OrderEvent event) {
        return orderEventEmitter.send(event);
    }

    public CompletionStage<Void> publishOrderUpdated(OrderEvent event) {
        return orderEventEmitter.send(event);
    }
}
