//! Generated C++ adapters for copied values exposed as Terlan strings.

use crate::commands::bind::cpp_binding_generator::error::CppBindingError;
use std::collections::BTreeMap;
use std::path::Path;

use super::{
    terlan_type_matches, CppSymbol, NativeBindingFunction, NativeBindingManifest,
    NativeBindingModule, NativeBindingType, NativeBindingTypeKind, NativeFunctionRole,
};

/// Returns the deterministic C++/Rust bridge name for one string projection.
pub(super) fn string_adapter_name(
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
) -> String {
    let owner = module
        .module
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("terlan_string_{owner}_{}", function.name)
}

/// Renders declarations for every generated copied-value string adapter.
pub(super) fn render_string_adapter_header(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, CppBindingError> {
    let header = Path::new(&manifest.cpp_metadata.header)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "C++ metadata header requires a UTF-8 file name".to_string())?;
    let mut source = format!(
        "#pragma once\n\n#include \"{header}\"\n\n#include <memory>\n#include <string>\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    for module in &manifest.modules {
        for function in string_functions(module) {
            let (resource, _, _, _) = string_projection_parts(module, function, symbols)?;
            let resource_symbol = symbol_for_type(resource, symbols)?;
            source.push_str(&format!(
                "std::unique_ptr<std::string> {}(const {}& value) noexcept;\n",
                string_adapter_name(module, function),
                resource_symbol.overload_set
            ));
        }
    }
    source.push_str(&format!(
        "\n}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

/// Renders implementations that copy selected C++ values through stringifiers.
pub(super) fn render_string_adapter_source(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, CppBindingError> {
    let mut source = format!(
        "#include \"include/terlan_string_adapters.hpp\"\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    for module in &manifest.modules {
        for function in string_functions(module) {
            let (resource, _, getter, stringifier) =
                string_projection_parts(module, function, symbols)?;
            let resource_symbol = symbol_for_type(resource, symbols)?;
            source.push_str(&format!(
                "std::unique_ptr<std::string> {}(const {}& value) noexcept {{\n  try {{\n    const auto projected = value.{}();\n    return std::make_unique<std::string>(projected.{}());\n  }} catch (...) {{\n    return nullptr;\n  }}\n}}\n\n",
                string_adapter_name(module, function),
                resource_symbol.overload_set,
                getter.cpp_name,
                stringifier.cpp_name
            ));
        }
    }
    source.push_str(&format!(
        "}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

fn string_functions(module: &NativeBindingModule) -> impl Iterator<Item = &NativeBindingFunction> {
    module
        .functions
        .iter()
        .filter(|function| function.role == NativeFunctionRole::StringProjection)
}

fn string_projection_parts<'a>(
    module: &'a NativeBindingModule,
    function: &'a NativeBindingFunction,
    symbols: &'a BTreeMap<&str, &CppSymbol>,
) -> Result<
    (
        &'a NativeBindingType,
        &'a NativeBindingType,
        &'a CppSymbol,
        &'a CppSymbol,
    ),
    CppBindingError,
> {
    let resource = function
        .args
        .first()
        .and_then(|arg| {
            module.types.iter().find(|ty| {
                ty.kind == NativeBindingTypeKind::OpaqueResource
                    && terlan_type_matches(&arg.ty, &ty.name)
            })
        })
        .ok_or_else(|| format!("string projection `{}` has no resource", function.name))?;
    let value_type = module
        .types
        .iter()
        .find(|ty| {
            ty.kind == NativeBindingTypeKind::StringValue
                && terlan_type_matches(&function.returns, &ty.name)
        })
        .ok_or_else(|| format!("string projection `{}` has no string value", function.name))?;
    let getter = function
        .cpp_symbol
        .as_deref()
        .and_then(|id| symbols.get(id).copied())
        .ok_or_else(|| format!("string projection `{}` has no C++ getter", function.name))?;
    let stringifier = value_type
        .stringifier
        .as_deref()
        .and_then(|id| symbols.get(id).copied())
        .ok_or_else(|| {
            format!(
                "string projection `{}` has no C++ stringifier",
                function.name
            )
        })?;
    Ok((resource, value_type, getter, stringifier))
}

fn symbol_for_type<'a>(
    ty: &NativeBindingType,
    symbols: &'a BTreeMap<&str, &CppSymbol>,
) -> Result<&'a CppSymbol, CppBindingError> {
    Ok(symbols
        .get(ty.cpp_symbol.as_str())
        .copied()
        .ok_or_else(|| format!("unknown C++ type symbol `{}`", ty.cpp_symbol))?)
}
