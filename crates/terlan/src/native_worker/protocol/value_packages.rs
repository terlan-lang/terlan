//! Value-only package execution behind the existing worker admission/lifecycle.

use terlan_runtime_abi::{BoundaryError, ErrorDomain, NativeBinding, NativeValue};

use crate::terlan_native_boundary::capability_wire::{CapabilityValue, MAX_CAPABILITY_TERM_COUNT};
use crate::terlan_native_boundary::request::RequestId;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};
use crate::terlan_native_boundary::worker::{NativeBoundaryWorker, NativeBoundaryWorkerReply};

use super::execution::CapabilityCall;

/// Executes an admitted nonblocking call with ordinary request credit accounting.
pub(super) fn call(
    worker: &mut NativeBoundaryWorker,
    binding: &NativeBinding,
    call: CapabilityCall,
) -> NativeBoundaryWorkerReply {
    let id = RequestId {
        value: call.request_id,
    };
    let result = match worker.begin_request(id) {
        Err(error) => error,
        Ok(()) => {
            let result = execute(binding, call.arguments).map_or_else(
                |error| NativeBoundaryReplyTerm::Error {
                    code: error.code().to_owned(),
                    message: error.to_string(),
                    offset: 0,
                },
                NativeBoundaryReplyTerm::Ok,
            );
            worker
                .finish_request(id)
                .map_or_else(|error| error, |()| result)
        }
    };
    NativeBoundaryWorkerReply {
        request_id: id,
        result,
        reserved_credits: worker.reserved_credits(),
        available_credits: worker.available_credits(),
    }
}

fn execute(
    binding: &NativeBinding,
    arguments: Vec<CapabilityValue>,
) -> Result<NativeBoundaryTerm, BoundaryError> {
    binding.validate_arity(arguments.len())?;
    let arguments = arguments
        .into_iter()
        .map(to_native)
        .collect::<Result<Vec<_>, _>>()?;
    let result = (binding.invoke)(&arguments)?;
    validate_result(&result)?;
    Ok(from_native(result))
}

fn invalid_value(message: &str) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "native package value",
        format!("error[native_package.value]: {message}"),
    )
}

/// Input frame decoding has already enforced recursion and term budgets.
fn to_native(value: CapabilityValue) -> Result<NativeValue, BoundaryError> {
    Ok(match value {
        CapabilityValue::Map(entries) => NativeValue::Map(
            entries
                .into_iter()
                .map(|(key, value)| Ok((to_native(key)?, to_native(value)?)))
                .collect::<Result<_, BoundaryError>>()?,
        ),
        CapabilityValue::Unit => NativeValue::Unit,
        CapabilityValue::Bool(value) => NativeValue::Bool(value),
        CapabilityValue::Int(value) => NativeValue::Int(value),
        CapabilityValue::Float(value) if value.is_finite() => NativeValue::Float(value),
        CapabilityValue::Float(_) => return Err(invalid_value("non-finite Float")),
        CapabilityValue::Text(value) => NativeValue::String(value),
        CapabilityValue::Bytes(value) => NativeValue::Bytes(value),
        CapabilityValue::Atom(value) => NativeValue::Atom(value),
        CapabilityValue::OptionalText(value) => value.into(),
        CapabilityValue::List(values) => NativeValue::List(
            values
                .into_iter()
                .map(to_native)
                .collect::<Result<_, _>>()?,
        ),
        CapabilityValue::Tuple(values) => NativeValue::Tuple(
            values
                .into_iter()
                .map(to_native)
                .collect::<Result<_, _>>()?,
        ),
        CapabilityValue::Record { name, fields } => NativeValue::Record {
            name,
            fields: fields
                .into_iter()
                .map(|(name, value)| Ok((name, to_native(value)?)))
                .collect::<Result<_, BoundaryError>>()?,
        },
        CapabilityValue::Handle(_)
        | CapabilityValue::OptionalHandle(_)
        | CapabilityValue::PostgresConfig(_) => {
            return Err(invalid_value(
                "value-only packages cannot accept resource identities or adapter configuration",
            ))
        }
    })
}

/// Check callback output before recursive encoding or JSON float serialization.
fn validate_result(value: &NativeValue) -> Result<(), BoundaryError> {
    let mut pending = vec![(value, 0)];
    let mut count = 0;
    while let Some((value, depth)) = pending.pop() {
        count += 1;
        if count > MAX_CAPABILITY_TERM_COUNT || depth > 64 {
            return Err(invalid_value(
                "package result exceeds the worker value budget",
            ));
        }
        match value {
            NativeValue::Float(value) if !value.is_finite() => {
                return Err(invalid_value("non-finite Float"));
            }
            NativeValue::List(values) | NativeValue::Tuple(values) => {
                pending.extend(values.iter().map(|value| (value, depth + 1)));
            }
            NativeValue::Map(entries) => pending.extend(
                entries
                    .iter()
                    .flat_map(|(key, value)| [(key, depth + 1), (value, depth + 1)]),
            ),
            NativeValue::Record { fields, .. } => {
                pending.extend(fields.iter().map(|(_, value)| (value, depth + 1)));
            }
            _ => {}
        }
    }
    Ok(())
}

fn from_native(value: NativeValue) -> NativeBoundaryTerm {
    match value {
        NativeValue::Map(entries) => NativeBoundaryTerm::Map(
            entries
                .into_iter()
                .map(|(key, value)| (from_native(key), from_native(value)))
                .collect(),
        ),
        NativeValue::Unit => NativeBoundaryTerm::Unit,
        NativeValue::Bool(value) => NativeBoundaryTerm::Bool(value),
        NativeValue::Int(value) => NativeBoundaryTerm::Int(value),
        NativeValue::Float(value) => NativeBoundaryTerm::Float(value),
        NativeValue::String(value) => NativeBoundaryTerm::Text(value),
        NativeValue::Bytes(value) => NativeBoundaryTerm::Bytes(value),
        NativeValue::Atom(value) => NativeBoundaryTerm::Atom(value),
        NativeValue::List(values) => {
            NativeBoundaryTerm::List(values.into_iter().map(from_native).collect())
        }
        NativeValue::Tuple(values) => {
            NativeBoundaryTerm::Tuple(values.into_iter().map(from_native).collect())
        }
        NativeValue::Record { name, fields } => NativeBoundaryTerm::Record {
            name,
            fields: fields
                .into_iter()
                .map(|(name, value)| (name, from_native(value)))
                .collect(),
        },
    }
}

#[cfg(test)]
#[path = "value_packages_test.rs"]
mod tests;
