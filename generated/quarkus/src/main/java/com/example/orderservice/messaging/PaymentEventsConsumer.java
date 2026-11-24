// GENERATED CODE — DO NOT EDIT.
// Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//   apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

package com.example.orderservice.messaging;

import com.example.orderservice.model.PaymentEvent;
import jakarta.enterprise.context.ApplicationScoped;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionStage;
import org.eclipse.microprofile.reactive.messaging.Incoming;

/** Consumes from channel `payment.events` (topic: payments). */
@ApplicationScoped
public class PaymentEventsConsumer {

    @Incoming("payment.events")
    public CompletionStage<Void> consumePaymentConfirmed(PaymentEvent event) {
        return handleConsumePaymentConfirmed(event);
    }

    @Incoming("payment.events")
    public CompletionStage<Void> consumePaymentRejected(PaymentEvent event) {
        return handleConsumePaymentRejected(event);
    }

    protected CompletionStage<Void> handleConsumePaymentConfirmed(PaymentEvent event) {
        // TODO implement business logic for consumePaymentConfirmed
        return CompletableFuture.completedFuture(null);
    }

    protected CompletionStage<Void> handleConsumePaymentRejected(PaymentEvent event) {
        // TODO implement business logic for consumePaymentRejected
        return CompletableFuture.completedFuture(null);
    }
}
