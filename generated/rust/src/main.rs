//! GENERATED CODE — DO NOT EDIT.
//! Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//!   apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

mod api;
mod messaging;
mod model;

use axum::Router;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .merge(api::customers::router())
        .merge(api::orders::router());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .expect("bind 127.0.0.1:8080");
    axum::serve(listener, app).await.expect("serve");
}
