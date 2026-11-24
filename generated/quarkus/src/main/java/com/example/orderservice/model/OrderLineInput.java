// GENERATED CODE — DO NOT EDIT.
// Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//   apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

package com.example.orderservice.model;

import jakarta.validation.constraints.DecimalMin;
import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotNull;
import java.util.UUID;

public record OrderLineInput(
        @NotNull UUID productId,
        @NotNull
        @Min(1)
        Integer quantity,
        @DecimalMin("0") Double unitPrice
) {
}
