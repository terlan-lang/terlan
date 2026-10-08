use crate::HttpError;
use cookie::Cookie;
pub use cookie::SameSite as CookieSameSite;
use time::{format_description::well_known::Rfc2822, Duration, OffsetDateTime};

/// Parses an HTTP request `Cookie` header into name/value pairs.
///
/// Inputs:
/// - `cookie_header`: raw request `Cookie` header value.
///
/// Output:
/// - Parsed cookie pairs in header order.
///
/// Transformation:
/// - Delegates cookie-pair parsing to the maintained `cookie` crate while
///   retaining the NativeBoundary-owned boundary used by the VM HTTP bridge.
pub fn parse_request_cookie_header(cookie_header: &str) -> Vec<(String, String)> {
    Cookie::split_parse(cookie_header.to_string())
        .filter_map(|cookie| {
            cookie.ok().and_then(|cookie| {
                (!cookie.name().trim().is_empty()).then(|| {
                    (
                        cookie.name().trim().to_string(),
                        cookie.value().trim().to_string(),
                    )
                })
            })
        })
        .collect()
}

/// Cookie options for `Set-Cookie` serialization.
///
/// Inputs:
/// - Produced by the source-selected cookie options at the package boundary.
///
/// Output:
/// - Stable option values consumed by `set_header_with_options`.
///
/// Transformation:
/// - Groups optional cookie metadata so validation and serialization can grow
///   without proliferating ad hoc function signatures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CookieOptions {
    /// Cookie path attribute.
    pub path: String,
    /// Optional cookie domain attribute.
    pub domain: Option<String>,
    /// Optional Max-Age attribute in seconds.
    pub max_age: Option<i64>,
    /// Optional Expires attribute text.
    pub expires: Option<String>,
    /// Whether to append `HttpOnly`.
    pub http_only: bool,
    /// Whether to append `Secure`.
    pub secure: bool,
    /// Optional SameSite policy.
    pub same_site: Option<CookieSameSite>,
}

/// Builds a `Set-Cookie` header value from typed cookie options.
///
/// Inputs:
/// - `name`: cookie name.
/// - `value`: cookie value.
/// - `options`: typed cookie metadata.
///
/// Output:
/// - Serialized `Set-Cookie` header value.
/// - `Err(HttpError)` when any option cannot be safely emitted.
///
/// Transformation:
/// - Validates the runtime's supported subset, then serializes with the
///   maintained `cookie` crate.
pub fn set_header_with_options(
    name: &str,
    value: &str,
    options: &CookieOptions,
) -> Result<String, HttpError> {
    validate_cookie_name(name)?;
    validate_cookie_value(value)?;
    validate_cookie_path(&options.path)?;
    if let Some(domain) = options.domain.as_deref() {
        validate_cookie_attribute("domain", domain)?;
    }
    if let Some(expires) = options.expires.as_deref() {
        validate_cookie_attribute("expires", expires)?;
    }

    let mut builder =
        Cookie::build((name.to_string(), value.to_string())).path(options.path.clone());
    if let Some(domain) = options.domain.as_deref() {
        builder = builder.domain(domain.to_string());
    }
    if let Some(max_age) = options.max_age {
        builder = builder.max_age(Duration::seconds(max_age));
    }
    if let Some(expires) = options.expires.as_deref() {
        builder = builder.expires(parse_cookie_expires(expires)?);
    }
    if options.http_only {
        builder = builder.http_only(true);
    }
    if options.secure {
        builder = builder.secure(true);
    }
    if let Some(same_site) = options.same_site {
        builder = builder.same_site(same_site);
    }
    Ok(builder.build().to_string())
}

/// Parses a cookie Expires option.
///
/// Inputs:
/// - `value`: RFC 2822/RFC 1123-style date string.
///
/// Output:
/// - Parsed UTC-aware date accepted by the cookie crate.
///
/// Transformation:
/// - Converts the source-visible string option into the maintained cookie
///   serializer's date-time type.
fn parse_cookie_expires(value: &str) -> Result<OffsetDateTime, HttpError> {
    OffsetDateTime::parse(value, &Rfc2822).map_err(|err| {
        HttpError::new(
            "http.cookie.invalid_attribute",
            format!("cookie expires attribute is not a supported HTTP date: {err}"),
            400,
        )
    })
}

/// Validates a cookie name.
///
/// Inputs:
/// - `name`: candidate cookie name.
///
/// Output:
/// - `Ok(())` when the name fits the conservative HTTP token subset.
/// - `Err(HttpError)` when the name is empty or contains unsupported bytes.
///
/// Transformation:
/// - Uses the maintained HTTP token parser while preserving the package's
///   existing rejection of dollar signs anywhere in cookie names.
fn validate_cookie_name(name: &str) -> Result<(), HttpError> {
    if name.contains('$') || http::HeaderName::from_bytes(name.as_bytes()).is_err() {
        return Err(HttpError::new(
            "http.cookie.invalid_name",
            format!("cookie name `{name}` is not supported"),
            400,
        ));
    }
    Ok(())
}

/// Validates a cookie value.
///
/// Inputs:
/// - `value`: candidate cookie value.
///
/// Output:
/// - `Ok(())` when the value can be emitted without quoting.
/// - `Err(HttpError)` when the value contains control or delimiter bytes.
///
/// Transformation:
/// - Keeps the first adapter surface intentionally strict so generated headers
///   cannot inject attributes or line breaks.
fn validate_cookie_value(value: &str) -> Result<(), HttpError> {
    if value
        .bytes()
        .any(|byte| byte < 0x21 || matches!(byte, b'"' | b',' | b';' | b'\\' | 0x7f))
    {
        return Err(HttpError::new(
            "http.cookie.invalid_value",
            "cookie value contains unsupported characters",
            400,
        ));
    }
    Ok(())
}

/// Validates a cookie path attribute.
///
/// Inputs:
/// - `path`: candidate cookie path.
///
/// Output:
/// - `Ok(())` when the path is absolute and safe to emit.
/// - `Err(HttpError)` when the path is empty, relative, or injects delimiters.
///
/// Transformation:
/// - Enforces the source-level convention that cookie paths are absolute URL
///   paths and leaves URL normalization to higher routing layers.
fn validate_cookie_path(path: &str) -> Result<(), HttpError> {
    if path.is_empty()
        || !path.starts_with('/')
        || path
            .bytes()
            .any(|byte| byte < 0x20 || matches!(byte, b';' | b'\r' | b'\n' | 0x7f))
    {
        return Err(HttpError::new(
            "http.cookie.invalid_path",
            format!("cookie path `{path}` is not supported"),
            400,
        ));
    }
    Ok(())
}

/// Validates a cookie attribute value.
///
/// Inputs:
/// - `name`: attribute name used for error reporting.
/// - `value`: candidate attribute value.
///
/// Output:
/// - `Ok(())` when the value is non-empty and safe to emit.
/// - `Err(HttpError)` when the value contains control characters or
///   delimiters.
///
/// Transformation:
/// - Applies the same conservative header-injection boundary to optional
///   cookie metadata before serialization.
fn validate_cookie_attribute(name: &str, value: &str) -> Result<(), HttpError> {
    if value.is_empty()
        || value
            .bytes()
            .any(|byte| byte < 0x20 || matches!(byte, b';' | b'\r' | b'\n' | 0x7f))
    {
        return Err(HttpError::new(
            "http.cookie.invalid_attribute",
            format!("cookie attribute `{name}` contains unsupported characters"),
            400,
        ));
    }
    Ok(())
}
