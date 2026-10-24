//! API discovery executes source; HTTP package code interprets the result.

use std::path::Path;

use terlan_http_native::{api_contract::ApiContract, source_descriptor};
use terlan_runtime_abi::NativeAdapterError;

use crate::commands::build::vm_artifact::source_exports::with_source_exports;
use crate::runtime::native_image::TvmExportDescriptor;
use crate::runtime::vm::native_callable::VmNativeCallableRef;
use crate::runtime::vm::ReplValue;

/// Compiles and evaluates a router without starting serving or granting services.
pub(crate) fn from_router_file(
    source: &Path,
    workspace: &Path,
    service_name: &str,
    version: &str,
) -> Result<ApiContract, String> {
    with_router_file(source, workspace, |module, router, exports| {
        ApiContract::from_source_router(service_name, version, router, |value| {
            callable_label(value, module, exports)
        })
        .map_err(String::from)
    })
}

/// Evaluates a source router for a package-owned projection while its image is loaded.
pub(crate) fn with_router_file<R>(
    source: &Path,
    workspace: &Path,
    project: impl FnOnce(
        &str,
        source_descriptor::Router<ReplValue>,
        &[TvmExportDescriptor],
    ) -> Result<R, String>,
) -> Result<R, String> {
    with_source_exports(source, workspace, &["router"], |module, values, exports| {
        let router = source_descriptor::router(&values[0], |callback, arity| {
            let actual = match callback {
                ReplValue::Closure(value) => Some(value.descriptor.parameters().len()),
                value => VmNativeCallableRef::from_value(value).map(|value| value.arity),
            };
            if actual != Some(arity) {
                return Err(NativeAdapterError::new(
                    "http.callback",
                    "invalid callback arity",
                    0,
                ));
            }
            Ok(callback.clone())
        })
        .map_err(|error| format!("error[{}]: {}", error.code(), error.message()))?;
        project(module, router, exports)
    })
    .map_err(|error| error.replacen("error[source_eval", "error[api_emit", 1))
}

pub(crate) fn callable_label(
    value: &ReplValue,
    module: &str,
    exports: &[TvmExportDescriptor],
) -> Result<String, String> {
    if let ReplValue::Closure(value) = value {
        if let Some(export) = exports
            .iter()
            .find(|export| export.id == value.descriptor.callable_id())
        {
            let qualified = export
                .name
                .rsplit_once('/')
                .map_or(export.name.as_str(), |(name, _)| name);
            return Ok(qualified
                .strip_prefix(&format!("{module}."))
                .unwrap_or(qualified)
                .to_string());
        }
        return Ok(format!("closure_{:016x}", value.descriptor.callable_id()));
    }
    let named = VmNativeCallableRef::from_value(value)
        .ok_or("error[api_emit]: invalid callable identity")?;
    Ok(if named.module == module {
        named.function
    } else {
        format!("{}.{}", named.module, named.function)
    })
}

#[cfg(test)]
#[path = "evaluated_contract_test.rs"]
mod tests;
