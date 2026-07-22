//! Generated C++ containment for explicitly selected throwing callables.

use std::collections::BTreeMap;
use std::path::Path;

use super::{
    terlan_type_matches, CppExceptionPolicy, CppSymbol, NativeBindingFunction,
    NativeBindingManifest, NativeBindingModule, NativeBindingType, NativeBindingTypeKind,
    NativeFunctionRole,
};

/// Opaque C++ result envelope shared by all contained calls in one package.
pub(super) const EXCEPTION_ENVELOPE: &str = "TerlanExceptionEnvelope";

/// Returns the deterministic bridge name for one contained C++ method.
pub(super) fn exception_adapter_name(
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
    format!("terlan_exception_{owner}_{}", function.name)
}

/// Renders the opaque result envelope and `noexcept` adapter declarations.
pub(super) fn render_exception_adapter_header(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let header = Path::new(&manifest.cpp_metadata.header)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "C++ metadata header requires a UTF-8 file name".to_string())?;
    let mut source = format!(
        "#pragma once\n\n#include \"{header}\"\n\n#include <cstdint>\n#include <memory>\n#include <string>\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    source.push_str(&format!(
        "class {EXCEPTION_ENVELOPE} final {{\n public:\n  {EXCEPTION_ENVELOPE}(bool ok, std::int64_t value, std::string code, std::string message) noexcept;\n  bool is_ok() const noexcept;\n  std::int64_t value() const noexcept;\n  const std::string& code() const noexcept;\n  const std::string& message() const noexcept;\n\n private:\n  bool ok_;\n  std::int64_t value_;\n  std::string code_;\n  std::string message_;\n}};\n\n"
    ));
    for module in &manifest.modules {
        for function in exception_functions(module) {
            let (resource, method, _) = exception_parts(manifest, module, function, symbols)?;
            let resource_symbol = symbol_for_type(resource, symbols)?;
            source.push_str(&format!(
                "std::unique_ptr<{EXCEPTION_ENVELOPE}> {}(const {}& value{}) noexcept;\n",
                exception_adapter_name(module, function),
                resource_symbol.cpp_name,
                cpp_parameters(method)
            ));
        }
    }
    source.push_str(&format!(
        "\n}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

/// Renders catch-all wrappers that suppress every upstream exception payload.
pub(super) fn render_exception_adapter_source(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let mut source = format!(
        "#include \"include/terlan_exception_adapters.hpp\"\n\n#include <utility>\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    source.push_str(&format!(
        "{EXCEPTION_ENVELOPE}::{EXCEPTION_ENVELOPE}(bool ok, std::int64_t value, std::string code, std::string message) noexcept\n    : ok_(ok), value_(value), code_(std::move(code)), message_(std::move(message)) {{}}\n\nbool {EXCEPTION_ENVELOPE}::is_ok() const noexcept {{ return ok_; }}\nstd::int64_t {EXCEPTION_ENVELOPE}::value() const noexcept {{ return value_; }}\nconst std::string& {EXCEPTION_ENVELOPE}::code() const noexcept {{ return code_; }}\nconst std::string& {EXCEPTION_ENVELOPE}::message() const noexcept {{ return message_; }}\n\n"
    ));
    for module in &manifest.modules {
        for function in exception_functions(module) {
            let (resource, method, policy) = exception_parts(manifest, module, function, symbols)?;
            let resource_symbol = symbol_for_type(resource, symbols)?;
            source.push_str(&format!(
                "std::unique_ptr<{EXCEPTION_ENVELOPE}> {}(const {}& value{}) noexcept {{\n  try {{\n    try {{\n      const auto result = value.{}({});\n      return std::make_unique<{EXCEPTION_ENVELOPE}>(true, result, \"\", \"\");\n    }} catch (...) {{\n      return std::make_unique<{EXCEPTION_ENVELOPE}>(false, 0, {:?}, {:?});\n    }}\n  }} catch (...) {{\n    return nullptr;\n  }}\n}}\n\n",
                exception_adapter_name(module, function),
                resource_symbol.cpp_name,
                cpp_parameters(method),
                method.cpp_name,
                method
                    .parameters
                    .iter()
                    .map(|parameter| parameter.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                policy.error_code,
                policy.message
            ));
        }
    }
    source.push_str(&format!(
        "}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

/// Returns exception-contained operations from one generated module.
fn exception_functions(
    module: &NativeBindingModule,
) -> impl Iterator<Item = &NativeBindingFunction> {
    module
        .functions
        .iter()
        .filter(|function| function.role == NativeFunctionRole::ExceptionMethod)
}

/// Resolves one method's resource, extractor declaration, and stable policy.
fn exception_parts<'a>(
    manifest: &'a NativeBindingManifest,
    module: &'a NativeBindingModule,
    function: &'a NativeBindingFunction,
    symbols: &'a BTreeMap<&str, &CppSymbol>,
) -> Result<(&'a NativeBindingType, &'a CppSymbol, &'a CppExceptionPolicy), String> {
    let resource = function
        .args
        .first()
        .and_then(|arg| {
            module.types.iter().find(|ty| {
                ty.kind == NativeBindingTypeKind::OpaqueResource
                    && terlan_type_matches(&arg.ty, &ty.name)
            })
        })
        .ok_or_else(|| format!("exception method `{}` has no resource", function.name))?;
    let symbol_id = function
        .cpp_symbol
        .as_deref()
        .ok_or_else(|| format!("exception method `{}` has no C++ symbol", function.name))?;
    let method = symbols
        .get(symbol_id)
        .copied()
        .ok_or_else(|| format!("unknown C++ exception method `{symbol_id}`"))?;
    let policy = manifest
        .mapping
        .symbols
        .iter()
        .find(|policy| policy.symbol == symbol_id)
        .and_then(|policy| policy.exception.as_ref())
        .ok_or_else(|| format!("exception method `{}` has no stable policy", function.name))?;
    Ok((resource, method, policy))
}

/// Renders validated integer parameters after the resource receiver.
fn cpp_parameters(symbol: &CppSymbol) -> String {
    symbol
        .parameters
        .iter()
        .map(|parameter| format!(", std::int64_t {}", parameter.name))
        .collect::<String>()
}

/// Resolves one generated resource's extractor-owned C++ declaration.
fn symbol_for_type<'a>(
    ty: &NativeBindingType,
    symbols: &'a BTreeMap<&str, &CppSymbol>,
) -> Result<&'a CppSymbol, String> {
    symbols
        .get(ty.cpp_symbol.as_str())
        .copied()
        .ok_or_else(|| format!("unknown C++ type symbol `{}`", ty.cpp_symbol))
}
