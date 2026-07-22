//! Typed security-policy lowering for managed HTTP responses.

use std::sync::Arc;

use crate::runtime::native_image::managed::{ManagedAggregateDescriptor, ManagedFieldType};
use crate::terlan_typeck::CoreExpr;

use super::{bool_expr, SECURITY_HEADERS};

/// Compiler-private constructor identity for a typed security policy.
pub(super) const SECURITY_CONSTRUCTOR: &str = "$terlan.http.security_headers";

/// Builds one typed security-policy value with secure fixed defaults.
pub(super) fn security_headers_constructor(
    hsts_max_age: i64,
    include_subdomains: bool,
) -> CoreExpr {
    CoreExpr::ConstructorCall {
        constructor: SECURITY_CONSTRUCTOR.to_string(),
        constructor_identity: Some(SECURITY_CONSTRUCTOR.to_string()),
        args: vec![
            bool_expr(true),
            CoreExpr::Int(0),
            CoreExpr::Int(1),
            CoreExpr::Int(hsts_max_age),
            bool_expr(include_subdomains),
        ],
    }
}

/// Lowers public security-policy atoms into the closed compiler-private ABI.
pub(super) fn lower_security_constructor_args(
    mut args: Vec<CoreExpr>,
) -> Result<Vec<CoreExpr>, String> {
    args[1] = security_marker(&args[1], &["Deny", "SameOrigin"])?;
    args[2] = security_marker(&args[2], &["NoReferrer", "StrictOriginWhenCrossOrigin"])?;
    Ok(args)
}

/// Builds the fixed typed security-policy descriptor consumed by the HTTP ABI.
pub(super) fn security_headers_descriptor() -> Result<Arc<ManagedAggregateDescriptor>, String> {
    ManagedAggregateDescriptor::tuple(
        SECURITY_HEADERS,
        vec![
            ManagedFieldType::Bool,
            ManagedFieldType::Int,
            ManagedFieldType::Int,
            ManagedFieldType::Int,
            ManagedFieldType::Bool,
        ],
    )
    .map(Arc::new)
    .map_err(|error| format!("error[native_ir.http_security_layout]: {error}"))
}

/// Converts one closed policy marker to its stable physical discriminant.
fn security_marker(value: &CoreExpr, variants: &[&str]) -> Result<CoreExpr, String> {
    if let CoreExpr::Int(value) = value {
        return Ok(CoreExpr::Int(*value));
    }
    let CoreExpr::Atom(value) = value else {
        return Err(
            "error[native_ir.http_security_policy]: policy markers must be closed atoms"
                .to_string(),
        );
    };
    variants
        .iter()
        .position(|variant| *variant == value)
        .map(|index| CoreExpr::Int(index as i64))
        .ok_or_else(|| {
            format!("error[native_ir.http_security_policy]: unsupported policy marker {value}")
        })
}
