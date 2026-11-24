// GENERATED CODE — DO NOT EDIT.
// Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//   apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

package com.example.orderservice.model;

import jakarta.validation.constraints.Email;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.util.UUID;

public record Customer(
        @NotNull UUID id,
        @NotNull
        @Size(min = 1, max = 100)
        String firstName,
        @NotNull
        @Size(min = 1, max = 100)
        String lastName,
        @NotNull
        @Email
        @Size(max = 255)
        String email,
        Address address
) {
}
