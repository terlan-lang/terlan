//! Maintained HTTP codecs owned by std.http, independent of compiler and VM.

#![forbid(unsafe_code)]

pub mod acme;
pub mod admitted_response;
pub mod api_contract;

mod bindings;
#[cfg(test)]
#[path = "tests/callbacks.rs"]
mod callback_test_support;
pub mod channel_admission;
mod channel_completion;
pub mod channel_plan;
mod content_type;
#[cfg(test)]
mod conversion_test;
mod cookies;
#[cfg(test)]
mod cookies_test;
mod error;
pub mod file_response;
pub mod http1;
pub mod http2;
#[cfg(test)]
mod http_test;
#[cfg(test)]
mod http_test_io;
pub mod lifecycle;
pub mod manifest;
pub mod plain_io;
mod request;
pub mod request_body;
pub mod request_ingress;
mod request_metadata;
pub mod request_pipeline;
mod request_projection;
pub mod request_resources;
mod request_value;
pub mod response_body;
mod response_builder;
mod response_chunks;
#[cfg(test)]
mod response_chunks_test;
mod response_headers;
#[cfg(test)]
mod response_metadata_test;
pub mod route_pattern;
pub mod routing;
pub mod server_config;
pub mod server_connection;
mod service_error;
pub mod session_bindings;
pub mod session_identity;
pub mod session_registry;
pub mod session_service;
pub mod session_store;
#[cfg(test)]
mod session_test_support;
pub mod source_descriptor;
mod sse;
pub mod sse_callbacks;
pub mod sse_session;
pub mod static_file;
pub mod tls;
pub mod tls_config;
pub use service_error::ServiceError;
pub mod tls_material;
pub mod tls_paths;
pub mod tls_runtime;
pub mod upgrade_io;
pub mod websocket;

pub use bindings::ENCODE_COOKIE;
pub use content_type::content_type_for_path;
pub use cookies::{
    parse_request_cookie_header, set_header_with_options, CookieOptions, CookieSameSite,
};
pub use error::HttpError;
pub use request::{Request, RequestMetadata, RequestParts};
pub use request_metadata::{query_pairs, request_cookie_pairs, request_header_pairs};
pub use request_projection::RequestFieldProjection;
pub use request_value::{request_descriptor, source_request_tuple};
pub use response_builder::{build_http_response, build_server_response, server_default_headers};
pub use response_chunks::{HttpResponseChunks, InvalidHttpStreamLimits};
pub use response_headers::validate_response_header;
pub use sse::{encode_event, ENCODE_EVENT};
