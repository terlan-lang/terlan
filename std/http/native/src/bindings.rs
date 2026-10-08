//! Package-owned cookie argument contracts and codec entry points.

use terlan_runtime_abi::{BoundaryError, ErrorDomain, FromNativeValue, NativeBinding, NativeValue};

use crate::{CookieOptions, CookieSameSite, HttpError};

/// Serializes the complete optional cookie attribute contract.
pub const ENCODE_COOKIE: NativeBinding = NativeBinding {
    operation: "std.http.cookies.encode",
    arity: 9,
    invoke: encode,
};

fn encode(args: &[NativeValue]) -> Result<NativeValue, BoundaryError> {
    ENCODE_COOKIE.validate_arity(args.len())?;
    let name = <&str>::from_native(&args[0])?;
    let value = <&str>::from_native(&args[1])?;
    let path = String::from_native(&args[2])?;
    let domain = Option::<String>::from_native(&args[3])?;
    let max_age = Option::<i64>::from_native(&args[4])?;
    let expires = Option::<String>::from_native(&args[5])?;
    let http_only = bool::from_native(&args[6])?;
    let secure = bool::from_native(&args[7])?;
    let same_site = match Option::<&str>::from_native(&args[8])? {
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
    crate::set_header_with_options(name, value, &options)
        .map(NativeValue::from)
        .map_err(codec_error)
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
