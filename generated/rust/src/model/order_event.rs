//! GENERATED CODE — DO NOT EDIT.
//! Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//!   python -m apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use super::*;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderEvent {
    pub event_id: Uuid,
    pub order_id: Uuid,
    pub type_: OrderEventType,
    pub occurred_at: DateTime<Utc>,
    pub order: Order,
}
