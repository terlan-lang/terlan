//! NativeBoundary adapter for the shared deterministic archive implementation.

use super::args::expect_text;
use super::{DispatchError, NativeBoundaryValue};

pub(super) fn dispatch(
    operation: &str,
    args: &[NativeBoundaryValue],
) -> Result<NativeBoundaryValue, DispatchError> {
    let first = expect_text(operation, args, 0)?;
    let second = expect_text(operation, args, 1)?;
    let result = match operation {
        "std.io.archive.create" => terlan_archive::create(first, second),
        "std.io.archive.extract" => terlan_archive::extract(first, second),
        _ => {
            return Err(DispatchError::new(
                "dispatch.unknown_operation",
                format!("archive dispatcher received unknown operation `{operation}`"),
                0,
            ));
        }
    };
    result
        .map(|()| NativeBoundaryValue::Unit)
        .map_err(|error| DispatchError::new(error.code(), error.to_string(), 0))
}
