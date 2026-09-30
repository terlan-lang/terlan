//! Compatibility imports for the package-owned std.http host adapter.

pub use terlan_http_native::{
    content_type_for_path, delete_header, file, header, html, json_text,
    parse_request_cookie_header, redirect, set_header, set_header_with_options, status, stream,
    text, CookieOptions, CookieSameSite, HttpError, Request, RequestMetadata, Response,
};
pub(crate) use terlan_http_native::{RequestFieldProjection, RequestParts};
