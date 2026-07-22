//! NativeBoundary manifest validation at resource-backed dispatch entry.

use std::sync::OnceLock;

use crate::terlan_native_boundary::metadata::{
    postgres_worker_manifest, NativeBoundaryExportManifest, NativeBoundaryWorkerClass,
};

use super::{DispatchError, NativeBoundaryBridgeValue};

static POSTGRES_MANIFEST_VALIDATION: OnceLock<Result<(), Vec<String>>> = OnceLock::new();

pub(super) fn validate_native_boundary_dispatch(
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
    granted_capabilities: Option<&[&str]>,
    admitted_worker_classes: Option<&[NativeBoundaryWorkerClass]>,
) -> Result<(), DispatchError> {
    if !operation.starts_with("std.db.postgres.") {
        return Ok(());
    }

    let manifest = postgres_worker_manifest();
    if let Err(diagnostics) = POSTGRES_MANIFEST_VALIDATION.get_or_init(|| manifest.validate()) {
        return Err(DispatchError::new(
            "native_boundary.invalid_manifest",
            format!(
                "NativeBoundary manifest `{}` is invalid: {}",
                manifest.adapter,
                diagnostics.join("; ")
            ),
            0,
        ));
    }
    let export = manifest.export_for_operation(operation).ok_or_else(|| {
        DispatchError::new(
            "native_boundary.missing_manifest",
            format!("No NativeBoundary manifest export exists for `{operation}`."),
            0,
        )
    })?;
    validate_capability(export, granted_capabilities)?;
    validate_scheduler_admission(export, admitted_worker_classes)?;
    validate_manifest_arity(export, args)?;
    validate_argument_shapes(export, args)
}

fn validate_scheduler_admission(
    export: &NativeBoundaryExportManifest,
    admitted_worker_classes: Option<&[NativeBoundaryWorkerClass]>,
) -> Result<(), DispatchError> {
    let Some(admitted_worker_classes) = admitted_worker_classes else {
        return Ok(());
    };
    if admitted_worker_classes.contains(&export.worker_class) {
        return Ok(());
    }
    Err(DispatchError::new(
        "native_boundary.scheduler_denied",
        format!(
            "NativeBoundary operation `{}` requires `{}` scheduler admission.",
            export.operation,
            worker_class_name(export.worker_class)
        ),
        0,
    ))
}

fn worker_class_name(worker_class: NativeBoundaryWorkerClass) -> &'static str {
    match worker_class {
        NativeBoundaryWorkerClass::Fast => "fast",
        NativeBoundaryWorkerClass::Blocking => "blocking",
        NativeBoundaryWorkerClass::LongRunningCancellable => "long-running-cancellable",
        NativeBoundaryWorkerClass::Sandboxed => "sandboxed",
        NativeBoundaryWorkerClass::ResourceOwning => "resource-owning",
    }
}

fn validate_capability(
    export: &NativeBoundaryExportManifest,
    granted_capabilities: Option<&[&str]>,
) -> Result<(), DispatchError> {
    let Some(granted_capabilities) = granted_capabilities else {
        return Ok(());
    };
    if granted_capabilities.contains(&export.required_capability) {
        return Ok(());
    }
    Err(DispatchError::new(
        "native_boundary.capability_denied",
        format!(
            "NativeBoundary operation `{}` requires capability `{}`.",
            export.operation, export.required_capability
        ),
        0,
    ))
}

fn validate_manifest_arity(
    export: &NativeBoundaryExportManifest,
    args: &[NativeBoundaryBridgeValue],
) -> Result<(), DispatchError> {
    if args.len() == export.arity {
        return Ok(());
    }
    Err(DispatchError::new(
        "native_boundary.arity",
        format!(
            "NativeBoundary operation `{}` expects {} arguments, received {}.",
            export.operation,
            export.arity,
            args.len()
        ),
        0,
    ))
}

fn validate_argument_shapes(
    export: &NativeBoundaryExportManifest,
    args: &[NativeBoundaryBridgeValue],
) -> Result<(), DispatchError> {
    for (index, (argument_type, argument)) in
        export.argument_types.iter().zip(args.iter()).enumerate()
    {
        if bridge_value_matches_type(argument, argument_type) {
            continue;
        }
        return Err(DispatchError::new(
            "native_boundary.argument_shape",
            format!(
                "NativeBoundary operation `{}` argument {index} must match `{argument_type}`.",
                export.operation
            ),
            0,
        ));
    }
    Ok(())
}

fn bridge_value_matches_type(value: &NativeBoundaryBridgeValue, argument_type: &str) -> bool {
    let mut delimiters = Vec::new();
    let mut member_start = 0;
    let mut saw_union = false;
    let mut matched = false;
    for (index, character) in argument_type.char_indices() {
        match character {
            '(' => delimiters.push(')'),
            '[' => delimiters.push(']'),
            '{' => delimiters.push('}'),
            ')' | ']' | '}' => {
                if delimiters.pop() != Some(character) {
                    return false;
                }
            }
            '|' if delimiters.is_empty() => {
                let member = argument_type[member_start..index].trim();
                if member.is_empty() {
                    return false;
                }
                saw_union = true;
                matched |= bridge_value_matches_atomic_type(value, member);
                member_start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    if !delimiters.is_empty() {
        return false;
    }
    if saw_union {
        let member = argument_type[member_start..].trim();
        return !member.is_empty() && (matched || bridge_value_matches_atomic_type(value, member));
    }

    bridge_value_matches_atomic_type(value, argument_type)
}

fn bridge_value_matches_atomic_type(
    value: &NativeBoundaryBridgeValue,
    argument_type: &str,
) -> bool {
    match argument_type {
        "std.db.Postgres.Config" => matches!(value, NativeBoundaryBridgeValue::PostgresConfig(_)),
        "Pool"
        | "Connection"
        | "Row"
        | "std.db.Postgres.Pool"
        | "std.db.Postgres.Connection"
        | "std.db.Postgres.Row" => {
            matches!(value, NativeBoundaryBridgeValue::Handle(_))
        }
        "String" => matches!(value, NativeBoundaryBridgeValue::Text(_)),
        "List[std.data.Json]" => matches!(
            value,
            NativeBoundaryBridgeValue::List(values)
                if values
                    .iter()
                    .all(|value| matches!(value, NativeBoundaryBridgeValue::Handle(_)))
        ),
        _ => false,
    }
}

#[cfg(test)]
#[path = "manifest_test.rs"]
mod manifest_test;
