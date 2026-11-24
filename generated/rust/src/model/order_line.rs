//! GENERATED CODE — DO NOT EDIT.
//! Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//!   apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderLine {
    pub product_id: Uuid,
    pub quantity: i32,
    pub unit_price: f64,
}
