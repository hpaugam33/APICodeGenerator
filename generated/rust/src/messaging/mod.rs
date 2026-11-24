//! GENERATED CODE — DO NOT EDIT.
//! Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//!   apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

use async_trait::async_trait;
use crate::model::*;

/// Producer for channel `order.events` (topic: orders) — implement with your broker client.
#[async_trait]
pub trait OrderEventsProducer {
    async fn publish_order_created(&self, event: OrderEvent) -> anyhow::Result<()>;
    async fn publish_order_updated(&self, event: OrderEvent) -> anyhow::Result<()>;
}

/// Consumer for channel `payment.events` (topic: payments) — implement with your broker client.
#[async_trait]
pub trait PaymentEventsConsumer {
    async fn consume_payment_confirmed(&self, event: PaymentEvent) -> anyhow::Result<()>;
    async fn consume_payment_rejected(&self, event: PaymentEvent) -> anyhow::Result<()>;
}
