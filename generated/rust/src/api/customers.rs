//! GENERATED CODE — DO NOT EDIT.
//! Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//!   python -m apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

use crate::model::*;
use super::ApiError;
use axum::extract::{Path, Query};
use axum::routing;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct GetCustomerPath {
    #[serde(rename = "customerId")]
    pub customer_id: Uuid,
}


/// Register a new customer.
pub async fn create_customer(Json(body): Json<CreateCustomerRequest>) -> Result<Json<Customer>, ApiError> {
    todo!("implement createCustomer")
}
/// Get a customer by id.
pub async fn get_customer(Path(path_params): Path<GetCustomerPath>) -> Result<Json<Customer>, ApiError> {
    todo!("implement getCustomer")
}

pub fn router() -> Router {
    Router::new()
        .route("/customers", routing::post(create_customer))
        .route("/customers/{customerId}", routing::get(get_customer))
}
