//! Cryptographic native-boundary adapters.

use super::args::expect_text;
use super::{DispatchError, NativeBoundaryValue};

pub(super) fn digest_md5(
    operation: &str,
    args: &[NativeBoundaryValue],
) -> Result<NativeBoundaryValue, DispatchError> {
    let text = expect_text(operation, args, 0)?;
    Ok(NativeBoundaryValue::Text(
        crate::runtime::native::md5::digest(text),
    ))
}

pub(super) fn verify_ed25519(
    operation: &str,
    args: &[NativeBoundaryValue],
) -> Result<NativeBoundaryValue, DispatchError> {
    let public_key = expect_text(operation, args, 0)?;
    let payload = expect_text(operation, args, 1)?;
    let signature = expect_text(operation, args, 2)?;
    Ok(NativeBoundaryValue::Bool(
        crate::runtime::native::ed25519::verify(public_key, payload, signature),
    ))
}
