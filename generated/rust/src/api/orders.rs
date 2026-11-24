//! GENERATED CODE — DO NOT EDIT.
//! Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//!   python -m apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

use crate::model::*;
use super::ApiError;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::routing;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ListOrdersQuery {
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub status: Option<OrderStatus>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteOrderPath {
    #[serde(rename = "orderId")]
    pub order_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct GetOrderPath {
    #[serde(rename = "orderId")]
    pub order_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct UpdateOrderStatusPath {
    #[serde(rename = "orderId")]
    pub order_id: Uuid,
}


/// List orders, optionally filtered by status (paginated).
pub async fn list_orders(Query(query_params): Query<ListOrdersQuery>) -> Result<Json<Vec<Order>>, ApiError> {
    todo!("implement listOrders")
}
/// Create an order for a customer.
pub async fn create_order(Json(body): Json<CreateOrderRequest>) -> Result<Json<Order>, ApiError> {
    todo!("implement createOrder")
}
/// Delete (cancel) an order.
pub async fn delete_order(Path(path_params): Path<DeleteOrderPath>) -> Result<StatusCode, ApiError> {
    todo!("implement deleteOrder")
}
/// Get an order by id.
pub async fn get_order(Path(path_params): Path<GetOrderPath>) -> Result<Json<Order>, ApiError> {
    todo!("implement getOrder")
}
/// Change the status of an order.
pub async fn update_order_status(Path(path_params): Path<UpdateOrderStatusPath>, Json(body): Json<UpdateOrderStatusRequest>) -> Result<Json<Order>, ApiError> {
    todo!("implement updateOrderStatus")
}

pub fn router() -> Router {
    Router::new()
        .route("/orders", routing::get(list_orders).post(create_order))
        .route("/orders/{orderId}", routing::get(get_order).delete(delete_order))
        .route("/orders/{orderId}/status", routing::patch(update_order_status))
}
