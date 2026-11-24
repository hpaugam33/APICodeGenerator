// GENERATED CODE — DO NOT EDIT.
// Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//   python -m apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

package com.example.orderservice.model;

import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Pattern;
import jakarta.validation.constraints.Size;

public record Address(
        @NotNull
        @Size(max = 255)
        String street,
        @NotNull
        @Size(max = 100)
        String city,
        @NotNull
        @Size(max = 20)
        String zipCode,
        @NotNull
        @Size(max = 2)
        @Pattern(regexp = "^[A-Z]{2}$")
        String country
) {
}
