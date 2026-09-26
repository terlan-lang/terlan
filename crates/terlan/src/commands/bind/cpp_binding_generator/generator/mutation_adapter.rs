//! Generated containment for throwing C++ free functions that mutate resources.

use crate::commands::bind::cpp_binding_generator::error::CppBindingError;
use std::collections::BTreeMap;

use super::exception_adapter::EXCEPTION_ENVELOPE;
use super::owned_value_adapter::{
    cpp_call_argument_with_presence, cpp_parameter, selected_overload_invocation,
    selected_overload_invocation_with_receiver,
};
use super::{
    borrowed_const_record_name, cpp_name_matches, function_symbol, mapped_cpp_parameter_count,
    public_cpp_parameter_mappings, terlan_type_matches, CppParameter, CppSymbol,
    NativeBindingFunction, NativeBindingManifest, NativeBindingModule, NativeBindingTypeKind,
    NativeFunctionRole,
};

/// Returns the collision-resistant bridge name for one contained mutation.
pub(super) fn mutation_adapter_name(
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
) -> String {
    let owner = module
        .module
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("terlan_mutation_{owner}_{}", function.name)
}

/// Returns whether one public mutation is routed through a generated contained
/// adapter. Mutable free functions always require it. A method opts in by
/// supplying the same finite exception policy; this also permits bridge-safe
/// lowering for logically-const C++ mutation APIs.
pub(super) fn function_uses_mutation_adapter(
    manifest: &NativeBindingManifest,
    function: &NativeBindingFunction,
) -> bool {
    if function.role == NativeFunctionRole::MutableFreeFunction {
        return true;
    }
    function.role == NativeFunctionRole::MutableMethod
        && function.cpp_symbol.as_deref().is_some_and(|symbol| {
            manifest
                .mapping
                .symbols
                .iter()
                .find(|policy| policy.symbol == symbol)
                .is_some_and(|policy| policy.exception.is_some())
        })
}

/// Returns whether this package needs any generated mutation adapters.
pub(super) fn has_mutation_adapters(manifest: &NativeBindingManifest) -> bool {
    manifest.modules.iter().any(|module| {
        module
            .functions
            .iter()
            .any(|function| function_uses_mutation_adapter(manifest, function))
    })
}

/// Renders declarations that expose only bridge-safe parameters and a contained result.
pub(super) fn render_mutation_adapter_header(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, CppBindingError> {
    let mut source = format!(
        "#pragma once\n\n#include \"terlan_exception_adapters.hpp\"\n#include \"terlan_owned_value_adapters.hpp\"\n\n#include <memory>\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    for module in &manifest.modules {
        for function in mutable_functions(manifest, module) {
            let symbol = function_symbol(function, symbols)?;
            source.push_str(&format!(
                "std::unique_ptr<{EXCEPTION_ENVELOPE}> {}({}) noexcept;\n",
                mutation_adapter_name(module, function),
                adapter_parameters(manifest, function, symbol)?
            ));
        }
    }
    source.push_str(&format!(
        "\n}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

/// Renders exact-call selection, exception containment, and alias-result disposal.
pub(super) fn render_mutation_adapter_source(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, CppBindingError> {
    let mut source = format!(
        "#include \"include/terlan_mutation_adapters.hpp\"\n\n#include <utility>\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    for module in &manifest.modules {
        for function in mutable_functions(manifest, module) {
            let symbol = function_symbol(function, symbols)?;
            let policy = manifest
                .mapping
                .symbols
                .iter()
                .find(|policy| policy.symbol == symbol.id)
                .and_then(|policy| policy.exception.as_ref())
                .ok_or_else(|| {
                    format!(
                        "mutable function `{}` has no stable exception policy",
                        function.name
                    )
                })?;
            let mappings = public_cpp_parameter_mappings(function, symbol);
            let count = mapped_cpp_parameter_count(function, symbol);
            let arguments = (0..count)
                .map(|index| {
                    let parameter = &symbol.parameters[index];
                    mappings
                        .iter()
                        .find(|mapping| mapping.cpp_parameter_index == index)
                        .map(|mapping| {
                            cpp_call_argument_with_presence(
                                manifest,
                                &parameter.name,
                                parameter,
                                mapping.public_type,
                                mapping.presence_name,
                                mapping.scalar_choice.as_ref(),
                            )
                        })
                        .unwrap_or_else(|| "std::nullopt".to_string())
                })
                .collect::<Vec<_>>()
                .join(", ");
            let (selection, invocation) = if symbol.overload_candidates > 1 {
                if symbol.kind == super::CppSymbolKind::Method {
                    selected_overload_invocation_with_receiver(symbol, &arguments, "self_value")?
                } else {
                    selected_overload_invocation(symbol, &arguments)?
                }
            } else if symbol.kind == super::CppSymbolKind::Method {
                (
                    String::new(),
                    format!("self_value.{}({arguments})", symbol.cpp_name),
                )
            } else {
                (
                    String::new(),
                    format!("{}({arguments})", symbol.overload_set),
                )
            };
            source.push_str(&format!(
                "std::unique_ptr<{EXCEPTION_ENVELOPE}> {}({}) noexcept {{\n  try {{\n    try {{\n{}      (void){};\n      return std::make_unique<{EXCEPTION_ENVELOPE}>(true, 0, 0.0, false, \"\", \"\");\n    }} catch (...) {{\n      return std::make_unique<{EXCEPTION_ENVELOPE}>(false, 0, 0.0, false, {:?}, {:?});\n    }}\n  }} catch (...) {{\n    return nullptr;\n  }}\n}}\n\n",
                mutation_adapter_name(module, function),
                adapter_parameters(manifest, function, symbol)?,
                selection,
                invocation,
                policy.error_code,
                policy.message,
            ));
        }
    }
    source.push_str(&format!(
        "}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

fn mutable_functions<'a>(
    manifest: &'a NativeBindingManifest,
    module: &'a NativeBindingModule,
) -> impl Iterator<Item = &'a NativeBindingFunction> {
    module
        .functions
        .iter()
        .filter(|function| function_uses_mutation_adapter(manifest, function))
}

fn adapter_parameters(
    manifest: &NativeBindingManifest,
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
) -> Result<String, CppBindingError> {
    let count = mapped_cpp_parameter_count(function, symbol);
    let mappings = public_cpp_parameter_mappings(function, symbol);
    if count > symbol.parameters.len() {
        return Err((format!(
            "mutable free function `{}` maps too many C++ parameters",
            function.name
        ))
        .into());
    }
    let mut parameters = Vec::new();
    if symbol.kind == super::CppSymbolKind::Method {
        let receiver = concrete_method_receiver(manifest, function, symbol)?;
        parameters.push(format!("{receiver}& self_value"));
    }
    for mapping in mappings {
        let index = mapping.cpp_parameter_index;
        let parameter = &symbol.parameters[index];
        if let Some(choice) = mapping.scalar_choice {
            parameters.push(format!("bool {}", choice.tag_name));
            parameters.push(format!("std::int64_t {}", choice.integer_name));
            parameters.push(format!("double {}", choice.floating_name));
            continue;
        }
        if let Some(presence) = mapping.presence_name {
            parameters.push(format!("bool {presence}"));
        }
        parameters.push(
            if symbol.kind != super::CppSymbolKind::Method && index == 0 {
                logical_mutation_target_parameter(manifest, parameter, mapping.public_type)
                    .unwrap_or_else(|| cpp_parameter(manifest, parameter, mapping.public_type))
            } else {
                cpp_parameter(manifest, parameter, mapping.public_type)
            },
        );
    }
    Ok(parameters.join(", "))
}

/// Resolves the concrete package resource used to invoke a method, preserving
/// Clang-proven public base conversion inside generated C++ instead of asking
/// the Rust CXX bridge to perform inheritance coercion.
fn concrete_method_receiver<'a>(
    manifest: &'a NativeBindingManifest,
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
) -> Result<&'a str, CppBindingError> {
    let argument = function.args.first().ok_or_else(|| {
        format!(
            "mutable method `{}` requires a resource receiver",
            function.name
        )
    })?;
    let resource = manifest
        .modules
        .iter()
        .flat_map(|module| &module.types)
        .find(|resource| {
            resource.kind == NativeBindingTypeKind::OpaqueResource
                && terlan_type_matches(&argument.ty, &resource.name)
        })
        .ok_or_else(|| {
            format!(
                "mutable method `{}` requires an opaque resource receiver",
                function.name
            )
        })?;
    Ok(manifest
        .cpp_metadata
        .symbols
        .iter()
        .find(|candidate| candidate.id == resource.cpp_symbol)
        .map(|candidate| candidate.overload_set.as_str())
        .ok_or_else(|| {
            format!(
                "mutable method `{}` has unknown resource symbol `{}` for `{}`",
                function.name, resource.cpp_symbol, symbol.id
            )
        })?)
}

/// Strengthens an upstream logical-const resource target to a mutable adapter
/// reference so CXX and the Rust handle table retain unique mutation semantics.
fn logical_mutation_target_parameter(
    manifest: &NativeBindingManifest,
    parameter: &CppParameter,
    public_type: Option<&str>,
) -> Option<String> {
    let record_name = borrowed_const_record_name(&parameter.ty)?;
    let resource = manifest
        .modules
        .iter()
        .flat_map(|module| &module.types)
        .find(|ty| {
            ty.kind == NativeBindingTypeKind::OpaqueResource
                && public_type.is_some_and(|public| terlan_type_matches(public, &ty.name))
        })?;
    let symbol = manifest
        .cpp_metadata
        .symbols
        .iter()
        .find(|symbol| symbol.id == resource.cpp_symbol)?;
    (cpp_name_matches(record_name, &symbol.cpp_name)
        || cpp_name_matches(record_name, &symbol.overload_set))
    .then(|| format!("{record_name} & {}", parameter.name))
}
