//! GENERATED CODE — DO NOT EDIT.
//! Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//!   apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

pub mod customers;
pub mod orders;

use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    NotImplemented,
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, "NOT_FOUND", m),
            ApiError::NotImplemented => (StatusCode::NOT_IMPLEMENTED, "NOT_IMPLEMENTED", "handler not implemented".to_string()),
            ApiError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", m),
        };
        (status, Json(ErrorBody { code: code.to_string(), message })).into_response()
    }
}
