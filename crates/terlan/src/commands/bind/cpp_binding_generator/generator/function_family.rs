use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::*;

/// Shared classification expanded into an explicit policy for every symbol.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CppSymbolPolicyFamily {
    /// Exact extracted symbols receiving this policy.
    symbols: Vec<String>,
    /// Shared bind or reject decision.
    disposition: CppSymbolDisposition,
    /// Shared ownership when the symbols are resource declarations.
    #[serde(default)]
    ownership: Option<CppOwnershipPolicy>,
    /// Shared thread-safety policy for resources.
    #[serde(default)]
    thread_safety: Option<CppThreadSafetyPolicy>,
    /// Shared rejection classification.
    #[serde(default)]
    rejection: Option<CppRejectionPolicy>,
    /// Shared stable exception policy.
    #[serde(default)]
    exception: Option<CppExceptionPolicy>,
}

/// Shared public contract for a homogeneous set of exact C++ declarations.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CppBindingFunctionFamily {
    /// Prefix joined with each member's operation suffix.
    operation_prefix: String,
    /// Controls whether expanded declarations are exported from the module.
    #[serde(default)]
    visibility: NativeVisibility,
    /// Generated call shape shared by every selected declaration.
    role: NativeFunctionRole,
    /// Public argument contract shared by the family.
    #[serde(default)]
    args: Vec<NativeBindingArg>,
    /// Public return type shared by the family.
    returns: String,
    /// Shared scheduling policy.
    blocking: NativeBlockingPolicy,
    /// Shared ownership policy.
    resource: NativeResourcePolicy,
    /// Exact extracted declarations expanded into ordinary functions.
    members: Vec<CppBindingFunctionFamilyMember>,
}

/// Per-declaration identity; names and docs default to extracted metadata.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CppBindingFunctionFamilyMember {
    /// Stable Clang declaration ID.
    cpp_symbol: String,
    /// Public function name; defaults to the extracted C++ name.
    #[serde(default)]
    name: Option<String>,
    /// Native operation suffix; defaults to the public name.
    #[serde(default)]
    operation_suffix: Option<String>,
    /// Public documentation; defaults to extracted declaration documentation.
    #[serde(default)]
    documentation: Option<String>,
}

/// Expands concise declaration families before the ordinary validation and
/// rendering pipeline. No C++ spelling is inferred from source text.
pub(super) fn expand_function_families(
    manifest: &mut NativeBindingManifest,
) -> Result<(), CppBindingError> {
    for family in std::mem::take(&mut manifest.mapping.symbol_families) {
        if family.symbols.is_empty() {
            return Err(
                ("error[cpp.symbol_family]: policy family has no symbols".to_string()).into(),
            );
        }
        for symbol in family.symbols {
            manifest.mapping.symbols.push(CppSymbolPolicy {
                symbol,
                disposition: family.disposition,
                ownership: family.ownership,
                thread_safety: family.thread_safety,
                rejection: family.rejection.clone(),
                exception: family.exception.clone(),
            });
        }
    }

    let declarations = manifest
        .cpp_metadata
        .symbols
        .iter()
        .map(|symbol| {
            (
                symbol.id.clone(),
                (symbol.cpp_name.clone(), symbol.documentation.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();

    for module in &mut manifest.modules {
        let mut names = module
            .functions
            .iter()
            .map(|function| function.name.clone())
            .collect::<BTreeSet<_>>();
        let mut operations = module
            .functions
            .iter()
            .map(|function| function.operation.clone())
            .collect::<BTreeSet<_>>();

        for family in std::mem::take(&mut module.function_families) {
            if family.operation_prefix.is_empty() || family.operation_prefix.ends_with('.') {
                return Err((format!(
                    "error[cpp.function_family]: module `{}` has invalid operation_prefix `{}`",
                    module.module, family.operation_prefix
                ))
                .into());
            }
            if family.members.is_empty() {
                return Err((format!(
                    "error[cpp.function_family]: module `{}` has a function family without members",
                    module.module
                ))
                .into());
            }
            if matches!(
                family.role,
                NativeFunctionRole::Dispose
                    | NativeFunctionRole::ValueProjection
                    | NativeFunctionRole::OwnedValueProjection
                    | NativeFunctionRole::ExceptionMethod
            ) {
                return Err((format!(
                    "error[cpp.function_family]: module `{}` uses unsupported family role `{:?}`",
                    module.module, family.role
                ))
                .into());
            }

            for member in family.members {
                let (cpp_name, extracted_documentation) = declarations
                    .get(&member.cpp_symbol)
                    .ok_or_else(|| {
                        format!(
                            "error[cpp.function_family]: module `{}` selects unknown C++ symbol `{}`",
                            module.module, member.cpp_symbol
                        )
                    })?;
                let name = member.name.unwrap_or_else(|| cpp_name.clone());
                let operation_suffix = member.operation_suffix.unwrap_or_else(|| name.clone());
                let operation = format!("{}.{}", family.operation_prefix, operation_suffix);
                if !names.insert(name.clone()) {
                    return Err((format!(
                        "error[cpp.function_family]: module `{}` repeats function `{name}`",
                        module.module
                    ))
                    .into());
                }
                if !operations.insert(operation.clone()) {
                    return Err((format!(
                        "error[cpp.function_family]: module `{}` repeats operation `{operation}`",
                        module.module
                    ))
                    .into());
                }
                module.functions.push(NativeBindingFunction {
                    name,
                    operation,
                    cpp_symbol: Some(member.cpp_symbol),
                    terlan_body: None,
                    visibility: family.visibility,
                    role: family.role,
                    args: family.args.clone(),
                    projections: Vec::new(),
                    fallible: None,
                    returns: family.returns.clone(),
                    blocking: family.blocking,
                    resource: family.resource,
                    documentation: member
                        .documentation
                        .unwrap_or_else(|| extracted_documentation.clone()),
                });
            }
        }
    }
    Ok(())
}
