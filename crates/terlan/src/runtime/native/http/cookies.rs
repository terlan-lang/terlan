use super::HttpError;
use cookie::{Cookie, SameSite};
use time::{format_description::well_known::Rfc2822, Duration, OffsetDateTime};

/// Request-scoped cookie jar used by the HTTP adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CookieJar {
    incoming: Vec<(String, String)>,
    mutations: Vec<String>,
}

impl CookieJar {
    /// Builds a cookie jar from parsed request cookies.
    ///
    /// Inputs:
    /// - `incoming`: decoded request cookie pairs.
    ///
    /// Output:
    /// - Cookie jar with no response mutations recorded yet.
    ///
    /// Transformation:
    /// - Stores incoming cookies separately from outgoing `Set-Cookie`
    ///   mutations so reads and writes remain explicit.
    pub fn from_pairs(incoming: Vec<(String, String)>) -> Self {
        Self {
            incoming,
            mutations: Vec::new(),
        }
    }

    /// Returns an incoming cookie value.
    ///
    /// Inputs:
    /// - `self`: cookie jar.
    /// - `name`: requested cookie name.
    ///
    /// Output:
    /// - `Some(value)` when an incoming cookie matches.
    /// - `None` when absent.
    ///
    /// Transformation:
    /// - Searches parsed request cookies without inspecting outgoing
    ///   mutations.
    pub fn get(&self, name: &str) -> Option<String> {
        self.incoming
            .iter()
            .find_map(|(key, value)| (key == name).then(|| value.clone()))
    }

    /// Records a response cookie mutation.
    ///
    /// Inputs:
    /// - `self`: mutable cookie jar.
    /// - `name`: cookie name.
    /// - `value`: cookie value.
    /// - `path`: cookie path attribute.
    /// - `http_only`: whether to append `HttpOnly`.
    /// - `secure`: whether to append `Secure`.
    ///
    /// Output:
    /// - `Ok(())` when the mutation is valid and recorded.
    /// - `Err(HttpError)` when validation fails.
    ///
    /// Transformation:
    /// - Serializes through the same conservative `Set-Cookie` helper used by
    ///   direct response headers and stores the header value for later response
    ///   application.
    pub fn set(
        &mut self,
        name: &str,
        value: &str,
        path: &str,
        http_only: bool,
        secure: bool,
    ) -> Result<(), HttpError> {
        let header = set_header(name, value, path, http_only, secure)?;
        self.mutations.push(header);
        Ok(())
    }

    /// Records a response cookie deletion.
    ///
    /// Inputs:
    /// - `self`: mutable cookie jar.
    /// - `name`: cookie name.
    /// - `path`: cookie path attribute.
    ///
    /// Output:
    /// - `Ok(())` when the deletion is valid and recorded.
    /// - `Err(HttpError)` when validation fails.
    ///
    /// Transformation:
    /// - Serializes a stable expiring `Set-Cookie` value and stores it as an
    ///   outgoing mutation.
    pub fn delete(&mut self, name: &str, path: &str) -> Result<(), HttpError> {
        let header = delete_header(name, path)?;
        self.mutations.push(header);
        Ok(())
    }

    /// Returns recorded `Set-Cookie` mutations.
    ///
    /// Inputs:
    /// - `self`: cookie jar.
    ///
    /// Output:
    /// - Borrowed mutation list in the order recorded.
    ///
    /// Transformation:
    /// - Exposes response-cookie metadata for the server bridge without
    ///   applying it to a concrete response type.
    pub fn mutations(&self) -> &[String] {
        &self.mutations
    }
}

/// SameSite cookie policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CookieSameSite {
    /// Lax SameSite policy.
    Lax,
    /// Strict SameSite policy.
    Strict,
    /// None SameSite policy.
    None,
}

impl CookieSameSite {
    /// Returns this policy in the maintained cookie crate's representation.
    ///
    /// Inputs:
    /// - `self`: cookie SameSite marker.
    ///
    /// Output:
    /// - Cookie crate SameSite value.
    ///
    /// Transformation:
    /// - Converts typed policy variants before response-cookie serialization.
    fn to_cookie_same_site(self) -> SameSite {
        match self {
            Self::Lax => SameSite::Lax,
            Self::Strict => SameSite::Strict,
            Self::None => SameSite::None,
        }
    }
}

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
///   retaining the SafeNative-owned boundary used by the BEAM HTTP bridge.
pub fn parse_request_cookie_header(cookie_header: &str) -> Vec<(String, String)> {
    cookie_header
        .split(';')
        .filter_map(|pair| match Cookie::parse(pair.trim().to_string()) {
            Ok(cookie) if !cookie.name().trim().is_empty() => Some((
                cookie.name().trim().to_string(),
                cookie.value().trim().to_string(),
            )),
            _ => None,
        })
        .collect()
}

/// Cookie options for `Set-Cookie` serialization.
///
/// Inputs:
/// - Produced by current helper defaults or future typed cookie option
///   lowering.
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

impl CookieOptions {
    /// Builds default cookie options for the current public helper.
    ///
    /// Inputs:
    /// - No explicit input.
    ///
    /// Output:
    /// - Cookie options with path `/` and all optional attributes absent.
    ///
    /// Transformation:
    /// - Centralizes default cookie option values so legacy and future helpers
    ///   serialize through the same validation path.
    pub fn defaults() -> Self {
        Self {
            path: "/".to_string(),
            domain: None,
            max_age: None,
            expires: None,
            http_only: false,
            secure: false,
            same_site: None,
        }
    }
}

/// Builds a conservative `Set-Cookie` header value.
///
/// Inputs:
/// - `name`: cookie name.
/// - `value`: cookie value.
/// - `path`: cookie path attribute.
/// - `http_only`: whether to append `HttpOnly`.
/// - `secure`: whether to append `Secure`.
///
/// Output:
/// - Serialized `Set-Cookie` header value.
/// - `Err(HttpError)` when name, value, or path cannot be safely emitted.
///
/// Transformation:
/// - Validates the practical cookie subset needed by the first Terlan HTTP
///   runtime and serializes it without exposing handler code to header
///   assembly details.
pub fn set_header(
    name: &str,
    value: &str,
    path: &str,
    http_only: bool,
    secure: bool,
) -> Result<String, HttpError> {
    let mut options = CookieOptions::defaults();
    options.path = path.to_string();
    options.http_only = http_only;
    options.secure = secure;
    set_header_with_options(name, value, &options)
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
        builder = builder.same_site(same_site.to_cookie_same_site());
    }
    Ok(builder.build().to_string())
}

/// Builds a conservative cookie deletion header value.
///
/// Inputs:
/// - `name`: cookie name.
/// - `path`: cookie path attribute.
///
/// Output:
/// - Serialized `Set-Cookie` header value that expires the cookie.
/// - `Err(HttpError)` when name or path cannot be safely emitted.
///
/// Transformation:
/// - Reuses the same validation as `set_header` and delegates serialization of
///   the `Max-Age=0` plus epoch `Expires` deletion shape to the maintained
///   cookie crate.
pub fn delete_header(name: &str, path: &str) -> Result<String, HttpError> {
    validate_cookie_name(name)?;
    validate_cookie_path(path)?;
    let expires = OffsetDateTime::from_unix_timestamp(0).map_err(|err| {
        HttpError::new(
            "http.cookie.invalid_attribute",
            format!("cookie deletion epoch could not be represented: {err}"),
            400,
        )
    })?;
    Ok(Cookie::build((name.to_string(), String::new()))
        .path(path.to_string())
        .max_age(Duration::seconds(0))
        .expires(expires)
        .build()
        .to_string())
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
/// - Applies the same practical token boundary used by response header names
///   while rejecting `$`-prefixed names reserved by cookie specifications.
fn validate_cookie_name(name: &str) -> Result<(), HttpError> {
    if name.is_empty()
        || name.starts_with('$')
        || !name.as_bytes().iter().copied().all(is_cookie_token_byte)
    {
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

/// Returns whether a byte is allowed in a conservative cookie token.
///
/// Inputs:
/// - `byte`: candidate byte.
///
/// Output:
/// - `true` when the byte is accepted.
///
/// Transformation:
/// - Implements the practical RFC token subset needed by cookie names.
fn is_cookie_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}
