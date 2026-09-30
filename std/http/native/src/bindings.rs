//! Package-owned cookie argument contracts and codec entry points.

use terlan_runtime_abi::{BoundaryError, ErrorDomain, NativeBinding, NativeValue};

use crate::{CookieOptions, CookieSameSite, HttpError};

/// Serializes a cookie with path and boolean attributes through the package codec.
pub const SET_HEADER: NativeBinding = NativeBinding {
    operation: "std.http.cookies.set_header",
    arity: 5,
    invoke: set_header,
};
/// Serializes the complete optional cookie attribute contract.
pub const SET_HEADER_WITH_OPTIONS: NativeBinding = NativeBinding {
    operation: "std.http.cookies.set_header_with_options",
    arity: 10,
    invoke: set_header_with_options,
};
/// Serializes an expired cookie for the requested name and path.
pub const DELETE_HEADER: NativeBinding = NativeBinding {
    operation: "std.http.cookies.delete_header",
    arity: 2,
    invoke: delete_header,
};

fn set_header(args: &[NativeValue]) -> Result<NativeValue, BoundaryError> {
    let [NativeValue::String(name), NativeValue::String(value), NativeValue::String(path), NativeValue::Bool(http_only), NativeValue::Bool(secure)] =
        args
    else {
        return Err(arguments(
            "set_header expects String, String, String, Bool, Bool",
        ));
    };
    crate::set_header(name, value, path, *http_only, *secure)
        .map(NativeValue::from)
        .map_err(codec_error)
}

fn set_header_with_options(args: &[NativeValue]) -> Result<NativeValue, BoundaryError> {
    let [NativeValue::String(name), NativeValue::String(value), NativeValue::String(path), NativeValue::String(domain), NativeValue::Int(max_age), NativeValue::Bool(include_max_age), NativeValue::String(expires), NativeValue::Bool(http_only), NativeValue::Bool(secure), NativeValue::String(same_site)] =
        args
    else {
        return Err(arguments(
            "set_header_with_options requires its ten typed cookie arguments",
        ));
    };
    let same_site = match same_site.to_ascii_lowercase().as_str() {
        "" => None,
        "lax" => Some(CookieSameSite::Lax),
        "strict" => Some(CookieSameSite::Strict),
        "none" => Some(CookieSameSite::None),
        other => {
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
        path: path.clone(),
        domain: (!domain.is_empty()).then(|| domain.clone()),
        max_age: include_max_age.then_some(*max_age),
        expires: (!expires.is_empty()).then(|| expires.clone()),
        http_only: *http_only,
        secure: *secure,
        same_site,
    };
    crate::set_header_with_options(name, value, &options)
        .map(NativeValue::from)
        .map_err(codec_error)
}

fn delete_header(args: &[NativeValue]) -> Result<NativeValue, BoundaryError> {
    let [NativeValue::String(name), NativeValue::String(path)] = args else {
        return Err(arguments("delete_header expects String, String"));
    };
    crate::delete_header(name, path)
        .map(NativeValue::from)
        .map_err(codec_error)
}

fn arguments(message: &str) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "cookie arguments",
        format!("error[dispatch.type]: {message}"),
    )
}

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
