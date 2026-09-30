//! Maintained HTTP codecs owned by std.http, independent of compiler and VM.

#![forbid(unsafe_code)]

mod bindings;
pub mod channel_plan;
#[cfg(test)]
mod conversion_test;
mod cookies;
#[cfg(test)]
mod cookies_test;
mod error;
pub mod http1;
#[cfg(test)]
mod http_test;
mod request;
mod request_metadata;
mod request_projection;
mod request_value;
mod response;
mod response_builder;
mod response_chunks;
#[cfg(test)]
mod response_chunks_test;
mod response_headers;
#[cfg(test)]
mod response_metadata_test;
pub mod source_descriptor;
mod sse;
pub mod tls;
pub mod websocket;

pub use bindings::{DELETE_HEADER, SET_HEADER, SET_HEADER_WITH_OPTIONS};
pub use cookies::{
    delete_header, parse_request_cookie_header, set_header, set_header_with_options, CookieOptions,
    CookieSameSite,
};
pub use error::HttpError;
pub use request::{Request, RequestMetadata, RequestParts};
pub use request_metadata::{query_pairs, request_cookie_pairs, request_header_pairs};
pub use request_projection::RequestFieldProjection;
pub use request_value::{request_descriptor, source_request_tuple};
pub use response::{
    content_type_for_path, file, header, html, json_text, redirect, status, stream, text, Response,
};
pub use response_builder::build_http_response;
pub use response_chunks::{HttpResponseChunks, InvalidHttpStreamLimits};
pub use response_headers::validate_response_header;
pub use sse::{encode_event, ENCODE_EVENT};
