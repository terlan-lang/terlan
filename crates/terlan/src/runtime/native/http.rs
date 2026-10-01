//! Compatibility imports for the package-owned std.http host adapter.

pub use terlan_http_native::{
    content_type_for_path, parse_request_cookie_header, set_header_with_options, CookieOptions,
    CookieSameSite, HttpError, Request, RequestMetadata,
};
pub(crate) use terlan_http_native::{RequestFieldProjection, RequestParts};
