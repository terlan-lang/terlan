//! Tooling evaluates ordinary source exports without interpreting package syntax.

use std::fs;
use std::path::Path;

use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::native_image::{host_tvm_target, inspect_tvm_image, TvmExportDescriptor};
use crate::runtime::vm::pure_native::{PureNativeExecution, PureNativeExecutionImage};
use crate::runtime::vm::ReplValue;

/// Compiles once and projects zero-argument exports while their image is loaded.
/// External waits are denied; the transition limit is not a native-code timeout.
pub(crate) fn with_source_exports<R>(
    source: &Path,
    workspace: &Path,
    functions: &[&str],
    project: impl FnOnce(&str, &[ReplValue], &[TvmExportDescriptor]) -> Result<R, String>,
) -> Result<R, String> {
    with_typed_source_exports(
        source,
        workspace,
        functions,
        |module, values, exports, _| project(module, values, exports),
    )
}

/// Retains checked type interfaces for consumers whose projection depends on field types.
pub(crate) fn with_typed_source_exports<R>(
    source: &Path,
    workspace: &Path,
    functions: &[&str],
    project: impl FnOnce(
        &str,
        &[ReplValue],
        &[TvmExportDescriptor],
        &[crate::terlan_hir::ModuleInterface],
    ) -> Result<R, String>,
) -> Result<R, String> {
    let source = source.canonicalize().map_err(|error| {
        format!(
            "error[source_eval]: cannot read {}: {error}",
            source.display()
        )
    })?;
    let text = fs::read_to_string(&source).map_err(|error| {
        format!(
            "error[source_eval]: cannot read {}: {error}",
            source.display()
        )
    })?;
    let syntax = crate::terlan_syntax::parse_module_as_syntax_output(&text)
        .map_err(|error| format!("error[source_eval]: cannot parse source: {error:?}"))?;
    let module = syntax.module_name;
    let compiled = super::compile_source_application(workspace, &source, &module)?;
    let image = PureNativeExecutionImage::load(&compiled.image.path)?;
    let inspection = inspect_tvm_image(
        &fs::read(&compiled.image.path).map_err(|error| error.to_string())?,
        &host_tvm_target()?.triple,
    )?;
    let exports = functions
        .iter()
        .map(|function| format!("{module}.{function}"))
        .collect::<Vec<_>>();
    for export in &exports {
        if !image.has_export(export, 0) {
            return Err(format!("error[source_eval]: missing public `{export}/0`"));
        }
    }
    let mut shard = image.spawn_shard()?;
    let mut values = Vec::with_capacity(exports.len());
    for export in exports {
        let (owner, mut execution) = shard.begin_call(&export, &[])?;
        let mut remaining = 1_024;
        loop {
            execution = match execution {
                PureNativeExecution::Complete(value) => {
                    shard.finish_completed_call(owner)?;
                    values.push(value);
                    break;
                }
                PureNativeExecution::Suspended(suspension) => {
                    if matches!(
                        suspension.operation(),
                        TvmTransitionOperation::Capability
                            | TvmTransitionOperation::Receive
                            | TvmTransitionOperation::Timer
                    ) {
                        let error = format!("error[source_eval.external_wait]: `{export}/0` requested {:?}; discovery grants no external services", suspension.operation());
                        shard.cancel_call(owner, error.clone())?;
                        return Err(error);
                    }
                    if remaining == 0 {
                        shard
                            .cancel_call(owner, "Source evaluation transition budget exhausted")?;
                        return Err(format!(
                            "error[source_eval.budget]: `{export}/0` exceeded 1024 execution transitions"
                        ));
                    }
                    remaining -= 1;
                    shard.resume_call(owner, *suspension)?
                }
            };
        }
    }
    project(
        &module,
        &values,
        &inspection.descriptor.exports,
        &compiled.interfaces,
    )
}

#[cfg(test)]
#[path = "source_exports_test.rs"]
mod tests;
