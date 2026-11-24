// GENERATED CODE — DO NOT EDIT.
// Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//   python -m apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

package com.example.orderservice.model;

import jakarta.validation.constraints.NotNull;
import java.time.OffsetDateTime;
import java.util.UUID;

public record OrderEvent(
        @NotNull UUID eventId,
        @NotNull UUID orderId,
        @NotNull OrderEventType type,
        @NotNull OffsetDateTime occurredAt,
        @NotNull Order order
) {
}
