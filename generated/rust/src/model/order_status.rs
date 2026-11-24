//! GENERATED CODE — DO NOT EDIT.
//! Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//!   apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

use serde::Deserialize;
use serde::Serialize;

/// Lifecycle of an order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatus {
    Pending,
    Confirmed,
    Shipped,
    Cancelled,
}
