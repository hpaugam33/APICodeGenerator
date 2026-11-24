// GENERATED CODE — DO NOT EDIT.
// Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//   python -m apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

package com.example.orderservice.model;

import jakarta.validation.constraints.DecimalMin;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.time.OffsetDateTime;
import java.util.List;
import java.util.UUID;

public record Order(
        @NotNull UUID id,
        @NotNull UUID customerId,
        @NotNull OrderStatus status,
        @NotNull
        @Size(min = 1)
        List<OrderLine> lines,
        @DecimalMin("0") Double totalAmount,
        @NotNull OffsetDateTime createdAt
) {
}
