//! GENERATED CODE — DO NOT EDIT.
//! Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//!   python -m apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

pub mod address;
pub mod create_customer_request;
pub mod create_order_request;
pub mod customer;
pub mod error;
pub mod order;
pub mod order_event;
pub mod order_event_type;
pub mod order_line;
pub mod order_line_input;
pub mod order_status;
pub mod payment_event;
pub mod payment_event_status;
pub mod update_order_status_request;

pub use address::*;
pub use create_customer_request::*;
pub use create_order_request::*;
pub use customer::*;
pub use error::*;
pub use order::*;
pub use order_event::*;
pub use order_event_type::*;
pub use order_line::*;
pub use order_line_input::*;
pub use order_status::*;
pub use payment_event::*;
pub use payment_event_status::*;
pub use update_order_status_request::*;
