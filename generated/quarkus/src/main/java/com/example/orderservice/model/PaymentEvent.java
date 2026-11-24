// GENERATED CODE — DO NOT EDIT.
// Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//   apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

package com.example.orderservice.model;

import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.time.OffsetDateTime;
import java.util.UUID;

public record PaymentEvent(
        @NotNull UUID eventId,
        @NotNull UUID orderId,
        @NotNull PaymentEventStatus status,
        @Size(max = 500) String reason,
        @NotNull OffsetDateTime occurredAt
) {
}
