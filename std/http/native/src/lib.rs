//! Maintained HTTP codecs owned by std.http, independent of compiler and VM.

#![forbid(unsafe_code)]

pub mod acme;

mod bindings;
pub mod channel_plan;
mod content_type;
#[cfg(test)]
mod conversion_test;
mod cookies;
#[cfg(test)]
mod cookies_test;
mod error;
pub mod http1;
pub mod http2;
#[cfg(test)]
mod http_test;
#[cfg(test)]
mod http_test_io;
pub mod lifecycle;
pub mod plain_io;
mod request;
pub mod request_body;
mod request_metadata;
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
pub mod session_bindings;
pub mod session_identity;
pub mod session_registry;
pub mod session_service;
pub mod session_store;
#[cfg(test)]
mod session_test_support;
pub mod source_descriptor;
mod sse;
pub mod sse_session;
pub mod tls;
pub mod tls_config;
pub mod tls_material;
pub mod tls_paths;
pub mod tls_runtime;
pub mod upgrade_io;
pub mod websocket;

pub use bindings::SET_HEADER_WITH_OPTIONS;
pub use content_type::content_type_for_path;
pub use cookies::{
    parse_request_cookie_header, set_header_with_options, CookieOptions, CookieSameSite,
};
pub use error::HttpError;
pub use request::{Request, RequestMetadata, RequestParts};
pub use request_metadata::{query_pairs, request_cookie_pairs, request_header_pairs};
pub use request_projection::RequestFieldProjection;
pub use request_value::{request_descriptor, source_request_tuple};
pub use response_builder::build_http_response;
pub use response_chunks::{HttpResponseChunks, InvalidHttpStreamLimits};
pub use response_headers::validate_response_header;
pub use sse::{encode_event, ENCODE_EVENT};
