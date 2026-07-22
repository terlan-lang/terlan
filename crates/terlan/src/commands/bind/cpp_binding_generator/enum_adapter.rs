//! Generated C++ adapters for finite symbolic enum results.

use std::collections::BTreeMap;
use std::path::Path;

use super::{
    terlan_type_matches, CppSymbol, NativeBindingFunction, NativeBindingManifest,
    NativeBindingModule, NativeBindingType, NativeBindingTypeKind, NativeFunctionRole,
};

/// Returns the deterministic C++/Rust bridge name for one enum projection.
pub(super) fn enum_adapter_name(
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
    format!("terlan_enum_{owner}_{}", function.name)
}

/// Renders declarations for every generated enum conversion adapter.
pub(super) fn render_enum_adapter_header(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let header = Path::new(&manifest.cpp_metadata.header)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "C++ metadata header requires a UTF-8 file name".to_string())?;
    let mut source = format!(
        "#pragma once\n\n#include \"{header}\"\n\n#include <memory>\n#include <string>\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    for module in &manifest.modules {
        for function in enum_functions(module) {
            let (resource, _, _) = enum_projection_parts(module, function, symbols)?;
            let resource_symbol = symbol_for_type(resource, symbols)?;
            source.push_str(&format!(
                "std::unique_ptr<std::string> {}(const {}& value) noexcept;\n",
                enum_adapter_name(module, function),
                resource_symbol.cpp_name
            ));
        }
    }
    source.push_str(&format!(
        "\n}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

/// Renders implementations that map named C++ enumerators to reviewed atoms.
pub(super) fn render_enum_adapter_source(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let mut source = format!(
        "#include \"include/terlan_enum_adapters.hpp\"\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    for module in &manifest.modules {
        for function in enum_functions(module) {
            let (resource, enum_type, getter) = enum_projection_parts(module, function, symbols)?;
            let resource_symbol = symbol_for_type(resource, symbols)?;
            let enum_symbol = symbol_for_type(enum_type, symbols)?;
            source.push_str(&format!(
                "std::unique_ptr<std::string> {}(const {}& value) noexcept {{\n  const auto result = value.{}();\n",
                enum_adapter_name(module, function),
                resource_symbol.cpp_name,
                getter.cpp_name
            ));
            for variant in &enum_type.variants {
                source.push_str(&format!(
                    "  if (result == {}::{}) {{\n    return std::make_unique<std::string>({:?});\n  }}\n",
                    enum_symbol.cpp_name, variant.cpp_name, variant.atom
                ));
            }
            source.push_str("  return nullptr;\n}\n\n");
        }
    }
    source.push_str(&format!(
        "}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

/// Returns all enum projections declared by one module.
fn enum_functions(module: &NativeBindingModule) -> impl Iterator<Item = &NativeBindingFunction> {
    module
        .functions
        .iter()
        .filter(|function| function.role == NativeFunctionRole::EnumProjection)
}

/// Resolves one projection's resource, result enum, and C++ getter.
fn enum_projection_parts<'a>(
    module: &'a NativeBindingModule,
    function: &'a NativeBindingFunction,
    symbols: &'a BTreeMap<&str, &CppSymbol>,
) -> Result<(&'a NativeBindingType, &'a NativeBindingType, &'a CppSymbol), String> {
    let resource = function
        .args
        .first()
        .and_then(|arg| {
            module.types.iter().find(|ty| {
                ty.kind == NativeBindingTypeKind::OpaqueResource
                    && terlan_type_matches(&arg.ty, &ty.name)
            })
        })
        .ok_or_else(|| format!("enum projection `{}` has no resource", function.name))?;
    let enum_type = module
        .types
        .iter()
        .find(|ty| {
            ty.kind == NativeBindingTypeKind::Enum
                && terlan_type_matches(&function.returns, &ty.name)
        })
        .ok_or_else(|| format!("enum projection `{}` has no enum result", function.name))?;
    let getter = function
        .cpp_symbol
        .as_deref()
        .and_then(|id| symbols.get(id).copied())
        .ok_or_else(|| format!("enum projection `{}` has no C++ getter", function.name))?;
    Ok((resource, enum_type, getter))
}

/// Resolves one generated type's extractor-owned C++ declaration.
fn symbol_for_type<'a>(
    ty: &NativeBindingType,
    symbols: &'a BTreeMap<&str, &CppSymbol>,
) -> Result<&'a CppSymbol, String> {
    symbols
        .get(ty.cpp_symbol.as_str())
        .copied()
        .ok_or_else(|| format!("unknown C++ type symbol `{}`", ty.cpp_symbol))
}
