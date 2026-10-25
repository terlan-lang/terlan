//! Package-owned cookie argument contracts and codec entry points.

use terlan_runtime_abi::{native_binding, BoundaryError, ErrorDomain, NativeBinding};

use crate::{CookieOptions, CookieSameSite, HttpError};

/// Decodes one source-selected Cookie header without selecting duplicate values.
pub const DECODE_COOKIE: NativeBinding =
    native_binding!("std.http.cookies.decode", |header: &str| {
        Ok::<_, BoundaryError>(crate::parse_request_cookie_header(header))
    });

/// Serializes the complete optional cookie attribute contract.
pub const ENCODE_COOKIE: NativeBinding = native_binding!(
    "std.http.cookies.encode",
    |name: &str,
     value: &str,
     path: String,
     domain: Option<String>,
     max_age: Option<i64>,
     expires: Option<String>,
     http_only: bool,
     secure: bool,
     same_site: Option<&str>| {
        let same_site = match same_site {
            None => None,
            Some("lax") => Some(CookieSameSite::Lax),
            Some("strict") => Some(CookieSameSite::Strict),
            Some("none") => Some(CookieSameSite::None),
            Some(other) => {
                return Err(BoundaryError::message(
                    ErrorDomain::NativeBoundary,
                    "cookie options",
                    format!(
                    "error[dispatch.http.cookie.invalid_same_site]: unsupported SameSite `{other}`"
                ),
                ))
            }
        };
        let options = CookieOptions {
            path,
            domain,
            max_age,
            expires,
            http_only,
            secure,
            same_site,
        };
        crate::set_header_with_options(name, value, &options).map_err(codec_error)
    }
);

fn codec_error(error: HttpError) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "cookie serialization",
        format!("error[{}]: {}", error.code(), error.message()),
    )
}

#[cfg(test)]
#[path = "bindings_test.rs"]
mod tests;
