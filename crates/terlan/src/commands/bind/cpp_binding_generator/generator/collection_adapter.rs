//! Generated C++ adapters that copy borrowed collection results.

use std::collections::BTreeMap;
use std::path::Path;

use super::{
    cpp_call_argument_with_presence, cpp_name_matches, cpp_parameters,
    public_cpp_parameter_mappings, selected_overload_invocation, terlan_type_matches, CppSymbol,
    CppSymbolKind, CppSymbolPolicy, NativeBindingFunction, NativeBindingManifest,
    NativeBindingModule, NativeBindingType, NativeBindingTypeKind, NativeFunctionRole,
    NativeResourcePolicy, ValidatedCppSymbols,
};

/// Returns the deterministic bridge name for one copied collection projection.
pub(super) fn collection_adapter_name(
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
    format!("terlan_collection_{owner}_{}", function.name)
}

/// Returns the deterministic bridge name for one owned resource-list adapter.
pub(super) fn resource_list_adapter_name(
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
) -> String {
    format!("{}_resources", collection_adapter_name(module, function))
}

/// Returns the generated opaque result class for one resource-list operation.
pub(super) fn resource_list_result_name(
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
) -> String {
    format!(
        "Terlan{}ResourceListResult",
        upper_camel(&format!("{}_{}", module.module, function.name))
    )
}

/// Renders declarations for borrowed integer collections copied into vectors.
pub(super) fn render_collection_adapter_header(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let header = Path::new(&manifest.cpp_metadata.header)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "C++ metadata header requires a UTF-8 file name".to_string())?;
    let mut source = format!(
        "#pragma once\n\n#include \"{header}\"\n#include \"terlan_owned_value_adapters.hpp\"\n#include \"rust/cxx.h\"\n\n#include <cstdint>\n#include <memory>\n#include <vector>\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    for module in &manifest.modules {
        for function in collection_functions(module) {
            let (resource, _) = collection_projection_parts(module, function, symbols)?;
            let resource_symbol = symbol_for_type(resource, symbols)?;
            source.push_str(&format!(
                "std::unique_ptr<std::vector<std::int64_t>> {}(const {}& value) noexcept;\n",
                collection_adapter_name(module, function),
                resource_symbol.overload_set
            ));
        }
        for function in resource_list_functions(module) {
            let (receiver, element, callable) =
                resource_list_parts(manifest, module, function, symbols)?;
            if let Some(receiver) = receiver {
                let _ = symbol_for_type(receiver, symbols)?;
            }
            let element_symbol = symbol_for_type(element, symbols)?;
            let result = resource_list_result_name(module, function);
            let adapter = resource_list_adapter_name(module, function);
            source.push_str(&format!(
                "class {result} final {{\n public:\n  explicit {result}(std::vector<{}> values) noexcept;\n  {result}(std::string code, std::string message) noexcept;\n  bool is_ok() const noexcept;\n  std::size_t len() const noexcept;\n  std::unique_ptr<{}> element(std::size_t index) const noexcept;\n  const std::string& code() const noexcept;\n  const std::string& message() const noexcept;\n\n private:\n  bool ok_;\n  std::vector<{}> values_;\n  std::string code_;\n  std::string message_;\n}};\n\nstd::unique_ptr<{result}> {adapter}({}) noexcept;\nbool {adapter}_is_ok(const {result}& result) noexcept;\nstd::size_t {adapter}_len(const {result}& result) noexcept;\nstd::unique_ptr<{}> {adapter}_element(const {result}& result, std::size_t index) noexcept;\nconst std::string& {adapter}_code(const {result}& result) noexcept;\nconst std::string& {adapter}_message(const {result}& result) noexcept;\n\n",
                element_symbol.overload_set,
                element_symbol.overload_set,
                element_symbol.overload_set,
                cpp_parameters(manifest, function, callable)?,
                element_symbol.overload_set,
            ));
        }
    }
    source.push_str(&format!(
        "\n}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

/// Renders catch-all adapters that immediately copy borrowed collection views.
pub(super) fn render_collection_adapter_source(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let mut source = format!(
        "#include \"include/terlan_collection_adapters.hpp\"\n\n#include <utility>\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    for module in &manifest.modules {
        for function in collection_functions(module) {
            let (resource, getter) = collection_projection_parts(module, function, symbols)?;
            let resource_symbol = symbol_for_type(resource, symbols)?;
            source.push_str(&format!(
                "std::unique_ptr<std::vector<std::int64_t>> {}(const {}& value) noexcept {{\n  try {{\n    const auto projected = value.{}();\n    return std::make_unique<std::vector<std::int64_t>>(projected.begin(), projected.end());\n  }} catch (...) {{\n    return nullptr;\n  }}\n}}\n\n",
                collection_adapter_name(module, function),
                resource_symbol.overload_set,
                getter.cpp_name
            ));
        }
        for function in resource_list_functions(module) {
            let (_receiver, element, callable) =
                resource_list_parts(manifest, module, function, symbols)?;
            let element_symbol = symbol_for_type(element, symbols)?;
            let result = resource_list_result_name(module, function);
            let adapter = resource_list_adapter_name(module, function);
            let mappings = public_cpp_parameter_mappings(function, callable);
            let parameter_count = super::mapped_cpp_parameter_count(function, callable);
            let arguments = callable
                .parameters
                .iter()
                .enumerate()
                .filter_map(|(index, parameter)| {
                    if let Some(mapping) = mappings
                        .iter()
                        .find(|mapping| mapping.cpp_parameter_index == index)
                    {
                        Some(cpp_call_argument_with_presence(
                            manifest,
                            &parameter.name,
                            parameter,
                            mapping.public_type,
                            mapping.presence_name,
                            mapping.scalar_choice.as_ref(),
                        ))
                    } else if index < parameter_count {
                        Some("std::nullopt".to_string())
                    } else if callable.overload_candidates > 1 {
                        Some(super::owned_value_adapter::cpp_default_argument_expression(
                            parameter,
                        ))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let (selection, invocation) = if callable.overload_candidates > 1 {
                selected_overload_invocation(callable, &arguments)?
            } else if callable.kind == CppSymbolKind::Method {
                (
                    String::new(),
                    format!("value.{}({arguments})", callable_name(callable)),
                )
            } else {
                (
                    String::new(),
                    format!("{}({arguments})", callable.overload_set),
                )
            };
            let policy = manifest
                .mapping
                .symbols
                .iter()
                .find(|policy| policy.symbol == callable.id)
                .and_then(|policy| policy.exception.as_ref())
                .ok_or_else(|| {
                    format!(
                        "resource-list projection `{}` has no exception policy",
                        function.name
                    )
                })?;
            source.push_str(&format!(
                "{result}::{result}(std::vector<{}> values) noexcept\n    : ok_(true), values_(std::move(values)) {{}}\n{result}::{result}(std::string code, std::string message) noexcept\n    : ok_(false), code_(std::move(code)), message_(std::move(message)) {{}}\nbool {result}::is_ok() const noexcept {{ return ok_; }}\nstd::size_t {result}::len() const noexcept {{ return values_.size(); }}\nstd::unique_ptr<{}> {result}::element(std::size_t index) const noexcept {{\n  try {{\n    if (index >= values_.size()) {{ return nullptr; }}\n    return std::make_unique<{}>(values_[index]);\n  }} catch (...) {{\n    return nullptr;\n  }}\n}}\nconst std::string& {result}::code() const noexcept {{ return code_; }}\nconst std::string& {result}::message() const noexcept {{ return message_; }}\n\nstd::unique_ptr<{result}> {adapter}({}) noexcept {{\n  try {{\n    try {{\n{selection}      return std::make_unique<{result}>({invocation});\n    }} catch (...) {{\n      return std::make_unique<{result}>({:?}, {:?});\n    }}\n  }} catch (...) {{\n    return nullptr;\n  }}\n}}\nbool {adapter}_is_ok(const {result}& result) noexcept {{ return result.is_ok(); }}\nstd::size_t {adapter}_len(const {result}& result) noexcept {{ return result.len(); }}\nstd::unique_ptr<{}> {adapter}_element(const {result}& result, std::size_t index) noexcept {{ return result.element(index); }}\nconst std::string& {adapter}_code(const {result}& result) noexcept {{ return result.code(); }}\nconst std::string& {adapter}_message(const {result}& result) noexcept {{ return result.message(); }}\n\n",
                element_symbol.overload_set,
                element_symbol.overload_set,
                element_symbol.overload_set,
                cpp_parameters(manifest, function, callable)?,
                policy.error_code,
                policy.message,
                element_symbol.overload_set,
            ));
        }
    }
    source.push_str(&format!(
        "}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

fn collection_functions(
    module: &NativeBindingModule,
) -> impl Iterator<Item = &NativeBindingFunction> {
    module
        .functions
        .iter()
        .filter(|function| function.role == NativeFunctionRole::IntListProjection)
}

fn resource_list_functions(
    module: &NativeBindingModule,
) -> impl Iterator<Item = &NativeBindingFunction> {
    module
        .functions
        .iter()
        .filter(|function| function.role == NativeFunctionRole::ResourceListProjection)
}

/// Resolves the receiver, returned element resource, and exact callable.
pub(super) fn resource_list_parts<'a>(
    manifest: &'a NativeBindingManifest,
    _module: &'a NativeBindingModule,
    function: &'a NativeBindingFunction,
    symbols: &'a BTreeMap<&str, &CppSymbol>,
) -> Result<
    (
        Option<&'a NativeBindingType>,
        &'a NativeBindingType,
        &'a CppSymbol,
    ),
    String,
> {
    let public_element = function
        .returns
        .strip_prefix("List[")
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| {
            format!(
                "resource-list projection `{}` must return a List",
                function.name
            )
        })?;
    let element = manifest
        .modules
        .iter()
        .flat_map(|owner| &owner.types)
        .find(|ty| {
            ty.kind == NativeBindingTypeKind::OpaqueResource
                && terlan_type_matches(public_element, &ty.name)
        })
        .ok_or_else(|| {
            format!(
                "resource-list projection `{}` has unknown element resource `{public_element}`",
                function.name
            )
        })?;
    let callable = function
        .cpp_symbol
        .as_deref()
        .and_then(|id| symbols.get(id).copied())
        .ok_or_else(|| {
            format!(
                "resource-list projection `{}` has no C++ callable",
                function.name
            )
        })?;
    let receiver = if callable.kind == CppSymbolKind::Method {
        Some(
            function
                .args
                .first()
                .and_then(|arg| {
                    manifest
                        .modules
                        .iter()
                        .flat_map(|owner| &owner.types)
                        .find(|ty| {
                            ty.kind == NativeBindingTypeKind::OpaqueResource
                                && terlan_type_matches(&arg.ty, &ty.name)
                        })
                })
                .ok_or_else(|| {
                    format!(
                        "resource-list projection `{}` has no resource receiver",
                        function.name
                    )
                })?,
        )
    } else {
        None
    };
    Ok((receiver, element, callable))
}

/// Validates one exception-contained vector of independently owned resources.
pub(super) fn validate_resource_list_projection(
    manifest: &NativeBindingManifest,
    function: &NativeBindingFunction,
    module: &NativeBindingModule,
    symbol: &CppSymbol,
    policy: &CppSymbolPolicy,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), String> {
    let (receiver, element, _) =
        resource_list_parts(manifest, module, function, &symbols.declarations)?;
    let receiver_symbol = receiver.map(|receiver| {
        symbols
            .declarations
            .get(receiver.cpp_symbol.as_str())
            .expect("validated resource-list receiver")
    });
    let element_symbol = symbols
        .declarations
        .get(element.cpp_symbol.as_str())
        .expect("validated resource-list element");
    let vector_element = symbol
        .returns
        .as_ref()
        .and_then(|returns| returns.canonical.strip_prefix("std::vector<"))
        .and_then(|value| value.strip_suffix('>'));
    if function.fallible.is_some()
        || function.resource != NativeResourcePolicy::OwnedHandle
        || policy.exception.is_none()
        || !matches!(symbol.kind, CppSymbolKind::Method | CppSymbolKind::Function)
        || symbol.kind == CppSymbolKind::Method
            && (symbol.receiver_mutable
                || !symbol.receiver.as_deref().is_some_and(|name| {
                    receiver_symbol
                        .is_some_and(|receiver| super::cpp_record_supports_receiver(receiver, name))
                }))
        || !vector_element.is_some_and(|name| cpp_name_matches(name, &element_symbol.cpp_name))
    {
        return Err(format!(
            "resource-list projection `{}` requires a contained const method or free function returning std::vector of a reviewed owned resource",
            function.name
        ));
    }
    super::validate_function_argument_mapping(function, symbol, &manifest.modules, symbols)
}

fn callable_name(symbol: &CppSymbol) -> String {
    if symbol.template_arguments.is_empty() {
        symbol.cpp_name.clone()
    } else {
        format!(
            "{}<{}>",
            symbol.cpp_name,
            symbol.template_arguments.join(", ")
        )
    }
}

fn upper_camel(value: &str) -> String {
    let mut result = String::new();
    let mut upper = true;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            result.push(if upper { ch.to_ascii_uppercase() } else { ch });
            upper = false;
        } else {
            upper = true;
        }
    }
    result
}

fn collection_projection_parts<'a>(
    module: &'a NativeBindingModule,
    function: &'a NativeBindingFunction,
    symbols: &'a BTreeMap<&str, &CppSymbol>,
) -> Result<(&'a NativeBindingType, &'a CppSymbol), String> {
    let resource = function
        .args
        .first()
        .and_then(|arg| {
            module.types.iter().find(|ty| {
                ty.kind == NativeBindingTypeKind::OpaqueResource
                    && terlan_type_matches(&arg.ty, &ty.name)
            })
        })
        .ok_or_else(|| {
            format!(
                "integer-list projection `{}` has no resource",
                function.name
            )
        })?;
    let getter = function
        .cpp_symbol
        .as_deref()
        .and_then(|id| symbols.get(id).copied())
        .ok_or_else(|| {
            format!(
                "integer-list projection `{}` has no C++ getter",
                function.name
            )
        })?;
    Ok((resource, getter))
}

fn symbol_for_type<'a>(
    ty: &NativeBindingType,
    symbols: &'a BTreeMap<&str, &CppSymbol>,
) -> Result<&'a CppSymbol, String> {
    symbols
        .get(ty.cpp_symbol.as_str())
        .copied()
        .ok_or_else(|| format!("unknown C++ type symbol `{}`", ty.cpp_symbol))
}
