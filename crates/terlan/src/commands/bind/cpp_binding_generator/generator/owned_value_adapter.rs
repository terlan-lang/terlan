//! Generated ownership adapters for opaque C++ resources returned by value.

use crate::commands::bind::cpp_binding_generator::error::CppBindingError;
use std::collections::BTreeMap;
use std::path::Path;

use super::{
    cpp_array_ref_element, cpp_name_matches, function_symbol, is_cpp_scalar_type,
    is_cpp_string_view_type, is_i64_array_ref_type, is_optional_cpp_scalar_type,
    is_optional_cpp_string_view_type, is_optional_i64_array_ref_type, is_rust_str_type,
    mapped_cpp_parameter_count, optional_cpp_inner_type, owned_unique_ptr_name,
    public_cpp_parameter_mappings, rust_bridge_type, terlan_type_matches, CppParameter, CppSymbol,
    CppTypeMetadata, NativeBindingFunction, NativeBindingManifest, NativeBindingModule,
    NativeBindingType, NativeBindingTypeKind, NativeFunctionRole, PublicScalarChoice,
};

/// Returns the generated CXX collector type for a list of opaque resources.
pub(super) fn resource_list_input_name(
    module: &NativeBindingModule,
    resource: &NativeBindingType,
) -> String {
    format!(
        "Terlan{}{}ResourceListInput",
        upper_camel(&module.module),
        upper_camel(&resource.name)
    )
}

pub(super) fn resource_list_input_new_name(
    module: &NativeBindingModule,
    resource: &NativeBindingType,
) -> String {
    format!(
        "terlan_resource_list_{}_{}_new",
        snake_ident(&module.module),
        snake_ident(&resource.name)
    )
}

pub(super) fn resource_list_input_push_name(
    module: &NativeBindingModule,
    resource: &NativeBindingType,
) -> String {
    format!(
        "terlan_resource_list_{}_{}_push",
        snake_ident(&module.module),
        snake_ident(&resource.name)
    )
}

/// Returns the collision-resistant bridge name for copying one resource input.
pub(super) fn resource_input_copy_name(
    module: &NativeBindingModule,
    resource: &NativeBindingType,
) -> String {
    format!(
        "terlan_resource_{}_{}_copy",
        snake_ident(&module.module),
        snake_ident(&resource.name)
    )
}

/// Returns the collision-resistant bridge name for one generated value adapter.
pub(super) fn owned_value_adapter_name(
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
    format!("terlan_owned_value_{owner}_{}", function.name)
}

/// Returns the collision-resistant carrier type for one structural tuple of
/// package-owned C++ resources. The carrier itself never crosses the public
/// Terlan boundary; CXX owns it only long enough to move out every element.
pub(super) fn owned_tuple_carrier_name(
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
) -> String {
    format!(
        "TerlanOwnedTuple{}{}",
        upper_camel(&module.module),
        upper_camel(&function.name)
    )
}

/// Returns the generated exact-call adapter for a resource tuple producer.
pub(super) fn owned_tuple_adapter_name(
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
) -> String {
    format!(
        "terlan_owned_tuple_{}_{}",
        snake_ident(&module.module),
        snake_ident(&function.name)
    )
}

/// Returns one generated move-out function for a tuple carrier element.
pub(super) fn owned_tuple_take_name(
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
    index: usize,
) -> String {
    format!(
        "{}_take_{index}",
        owned_tuple_adapter_name(module, function)
    )
}

/// Resolved public module, resource declaration and C++ symbol for a tuple element.
type OwnedTupleResource<'a> = (
    &'a NativeBindingModule,
    &'a NativeBindingType,
    &'a CppSymbol,
);

/// Resolves a flat structural Terlan tuple whose elements are all opaque
/// package resources and proves that the selected callable returns the same
/// ordered `std::tuple` of C++ values.
pub(super) fn function_owned_value_resource_tuple<'a>(
    manifest: &'a NativeBindingManifest,
    function: &NativeBindingFunction,
    symbols: &'a BTreeMap<&str, &'a CppSymbol>,
) -> Option<(Vec<OwnedTupleResource<'a>>, &'a CppSymbol)> {
    if !matches!(
        function.role,
        NativeFunctionRole::Constructor
            | NativeFunctionRole::FreeFunction
            | NativeFunctionRole::ImmutableMethod
    ) {
        return None;
    }
    let public_elements = structural_tuple_elements(&function.returns)?;
    let callable = function_symbol(function, symbols).ok()?;
    let resources =
        resource_tuple_return_resources(&manifest.modules, &function.returns, callable, symbols)?;
    (public_elements.len() >= 2).then_some((resources, callable))
}

/// Proves one public structural resource tuple matches the ordered element
/// types of an extracted C++ `std::tuple` result.
pub(super) fn resource_tuple_return_resources<'a>(
    modules: &'a [NativeBindingModule],
    public_return: &str,
    callable: &CppSymbol,
    symbols: &'a BTreeMap<&str, &'a CppSymbol>,
) -> Option<
    Vec<(
        &'a NativeBindingModule,
        &'a NativeBindingType,
        &'a CppSymbol,
    )>,
> {
    let public_elements = structural_tuple_elements(public_return)?;
    let cpp_elements = callable
        .returns
        .as_ref()
        .and_then(|returns| cpp_std_tuple_elements(&returns.canonical))?;
    if public_elements.len() != cpp_elements.len() {
        return None;
    }
    let resources = public_elements
        .into_iter()
        .zip(cpp_elements)
        .map(|(public, cpp)| {
            modules.iter().find_map(|module| {
                module.types.iter().find_map(|resource| {
                    if resource.kind != NativeBindingTypeKind::OpaqueResource
                        || !terlan_type_matches(public, &resource.name)
                    {
                        return None;
                    }
                    let symbol = symbols.get(resource.cpp_symbol.as_str()).copied()?;
                    (cpp_name_matches(cpp, &symbol.cpp_name)
                        || cpp_name_matches(cpp, &symbol.overload_set))
                    .then_some((module, resource, symbol))
                })
            })
        })
        .collect::<Option<Vec<_>>>()?;
    (resources.len() >= 2).then_some(resources)
}

/// Parses a flat structural Terlan tuple without confusing generic type commas
/// with tuple separators.
fn structural_tuple_elements(value: &str) -> Option<Vec<&str>> {
    split_top_level(value.strip_prefix('{')?.strip_suffix('}')?, '[', ']')
}

/// Parses an extracted `std::tuple<T...>` canonical spelling.
pub(super) fn cpp_std_tuple_elements(value: &str) -> Option<Vec<&str>> {
    let value = value.trim();
    split_top_level(
        value.strip_prefix("std::tuple<")?.strip_suffix('>')?,
        '<',
        '>',
    )
}

/// Splits a non-empty, at-least-two-element list while respecting one nested
/// delimiter family. This is sufficient for canonical C++ template spellings
/// and public Terlan generic element types without accepting malformed input.
fn split_top_level(value: &str, open: char, close: char) -> Option<Vec<&str>> {
    let mut depth = 0_usize;
    let mut start = 0_usize;
    let mut elements = Vec::new();
    for (index, character) in value.char_indices() {
        if character == open {
            depth = depth.checked_add(1)?;
        } else if character == close {
            depth = depth.checked_sub(1)?;
        } else if character == ',' && depth == 0 {
            elements.push(value[start..index].trim());
            start = index + character.len_utf8();
        }
    }
    if depth != 0 {
        return None;
    }
    elements.push(value[start..].trim());
    (elements.len() >= 2 && elements.iter().all(|element| !element.is_empty())).then_some(elements)
}

/// Returns whether a function needs generated by-value to `unique_ptr` adaptation.
pub(super) fn function_owned_value_resource<'a>(
    manifest: &'a NativeBindingManifest,
    function: &NativeBindingFunction,
    symbols: &'a BTreeMap<&str, &CppSymbol>,
) -> Option<(&'a NativeBindingType, &'a CppSymbol)> {
    if !matches!(
        function.role,
        NativeFunctionRole::Constructor
            | NativeFunctionRole::FreeFunction
            | NativeFunctionRole::ImmutableMethod
    ) {
        return None;
    }
    let resource = manifest
        .modules
        .iter()
        .flat_map(|module| &module.types)
        .find(|ty| {
            ty.kind == NativeBindingTypeKind::OpaqueResource
                && terlan_type_matches(&function.returns, &ty.name)
        })?;
    let resource_symbol = symbols.get(resource.cpp_symbol.as_str()).copied()?;
    let callable = function_symbol(function, symbols).ok()?;
    let returns_resource_by_value = callable
        .returns
        .as_ref()
        .is_some_and(|returns| cpp_name_matches(&returns.canonical, &resource_symbol.cpp_name));
    let returns_resource_by_pointer = callable
        .returns
        .as_ref()
        .and_then(owned_unique_ptr_name)
        .is_some_and(|name| cpp_name_matches(name, &resource_symbol.cpp_name));
    let requires_resource_list_adapter = public_cpp_parameter_mappings(function, callable)
        .into_iter()
        .any(|mapping| {
            resource_list_input_resource(
                manifest,
                mapping.public_type,
                &callable.parameters[mapping.cpp_parameter_index].ty,
            )
            .is_some()
        });
    (returns_resource_by_value || (returns_resource_by_pointer && requires_resource_list_adapter))
        .then_some((resource, callable))
}

/// Returns whether this package requires any generated opaque-value adapters.
pub(super) fn has_owned_value_adapters(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> bool {
    !used_mutable_secondary_resources(manifest, symbols).is_empty()
        || manifest.modules.iter().any(|module| {
            module.functions.iter().any(|function| {
                function_owned_value_resource(manifest, function, symbols).is_some()
                    || function_owned_value_resource_tuple(manifest, function, symbols).is_some()
            })
        })
}

/// Renders generated adapter declarations for every selected by-value resource.
pub(super) fn render_owned_value_adapter_header(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, CppBindingError> {
    let header = Path::new(&manifest.cpp_metadata.header)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "C++ metadata header requires a UTF-8 file name".to_string())?;
    let mut source = format!(
        "#pragma once\n\n#include \"{header}\"\n#include \"rust/cxx.h\"\n\n#include <cstdint>\n#include <memory>\n#include <tuple>\n#include <utility>\n#include <vector>\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    for (module, resource, symbol) in used_resource_list_inputs(manifest, symbols) {
        let collector = resource_list_input_name(module, resource);
        let new = resource_list_input_new_name(module, resource);
        let push = resource_list_input_push_name(module, resource);
        source.push_str(&format!(
            "class {collector} final {{\n public:\n  bool push(const {}& value) noexcept;\n  const std::vector<{}>& values() const noexcept;\n\n private:\n  std::vector<{}> values_;\n}};\nstd::unique_ptr<{collector}> {new}() noexcept;\nbool {push}({collector}& values, const {}& value) noexcept;\n\n",
            symbol.overload_set,
            symbol.overload_set,
            symbol.overload_set,
            symbol.overload_set,
        ));
    }
    for (module, resource, symbol) in used_mutable_secondary_resources(manifest, symbols) {
        source.push_str(&format!(
            "std::unique_ptr<{}> {}(const {}& value) noexcept;\n",
            symbol.overload_set,
            resource_input_copy_name(module, resource),
            symbol.overload_set,
        ));
    }
    for module in &manifest.modules {
        for function in &module.functions {
            let Some((resource, callable)) =
                function_owned_value_resource(manifest, function, symbols)
            else {
                continue;
            };
            let resource_symbol = symbols
                .get(resource.cpp_symbol.as_str())
                .ok_or_else(|| format!("unknown C++ type symbol `{}`", resource.cpp_symbol))?;
            source.push_str(&format!(
                "std::unique_ptr<{}> {}({}) noexcept;\n",
                resource_symbol.overload_set,
                owned_value_adapter_name(module, function),
                cpp_parameters(manifest, function, callable)?
            ));
        }
    }
    for module in &manifest.modules {
        for function in &module.functions {
            let Some((resources, callable)) =
                function_owned_value_resource_tuple(manifest, function, symbols)
            else {
                continue;
            };
            let carrier = owned_tuple_carrier_name(module, function);
            let constructor_parameters = resources
                .iter()
                .enumerate()
                .map(|(index, (_, _, symbol))| {
                    format!("std::unique_ptr<{}> value_{index}", symbol.overload_set)
                })
                .collect::<Vec<_>>()
                .join(", ");
            let initializers = resources
                .iter()
                .enumerate()
                .map(|(index, _)| format!("value_{index}_(std::move(value_{index}))"))
                .collect::<Vec<_>>()
                .join(", ");
            source.push_str(&format!(
                "class {carrier} final {{\n public:\n  explicit {carrier}({constructor_parameters}) noexcept : {initializers} {{}}\n"
            ));
            for (index, (_, _, symbol)) in resources.iter().enumerate() {
                source.push_str(&format!(
                    "  std::unique_ptr<{}> take_{index}() noexcept {{ return std::move(value_{index}_); }}\n",
                    symbol.overload_set
                ));
            }
            source.push_str("\n private:\n");
            for (index, (_, _, symbol)) in resources.iter().enumerate() {
                source.push_str(&format!(
                    "  std::unique_ptr<{}> value_{index}_;\n",
                    symbol.overload_set
                ));
            }
            source.push_str("};\n");
            source.push_str(&format!(
                "std::unique_ptr<{carrier}> {}({}) noexcept;\n",
                owned_tuple_adapter_name(module, function),
                cpp_parameters(manifest, function, callable)?
            ));
            for (index, (_, _, symbol)) in resources.iter().enumerate() {
                source.push_str(&format!(
                    "std::unique_ptr<{}> {}({carrier}& value) noexcept;\n",
                    symbol.overload_set,
                    owned_tuple_take_name(module, function, index)
                ));
            }
            source.push('\n');
        }
    }
    source.push_str(&format!(
        "\n}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

/// Renders catch-all adapters that own C++ resource values without handwritten glue.
pub(super) fn render_owned_value_adapter_source(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, CppBindingError> {
    let mut source = format!(
        "#include \"include/terlan_owned_value_adapters.hpp\"\n\n#include <string>\n#include <utility>\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    for (module, resource, symbol) in used_resource_list_inputs(manifest, symbols) {
        let collector = resource_list_input_name(module, resource);
        let new = resource_list_input_new_name(module, resource);
        let push = resource_list_input_push_name(module, resource);
        source.push_str(&format!(
            "bool {collector}::push(const {}& value) noexcept {{\n  try {{ values_.push_back(value); return true; }} catch (...) {{ return false; }}\n}}\nconst std::vector<{}>& {collector}::values() const noexcept {{ return values_; }}\nstd::unique_ptr<{collector}> {new}() noexcept {{\n  try {{ return std::make_unique<{collector}>(); }} catch (...) {{ return nullptr; }}\n}}\nbool {push}({collector}& values, const {}& value) noexcept {{ return values.push(value); }}\n\n",
            symbol.overload_set,
            symbol.overload_set,
            symbol.overload_set,
        ));
    }
    for (module, resource, symbol) in used_mutable_secondary_resources(manifest, symbols) {
        source.push_str(&format!(
            "std::unique_ptr<{}> {}(const {}& value) noexcept {{\n  try {{ return std::make_unique<{}>(value); }} catch (...) {{ return nullptr; }}\n}}\n\n",
            symbol.overload_set,
            resource_input_copy_name(module, resource),
            symbol.overload_set,
            symbol.overload_set,
        ));
    }
    for module in &manifest.modules {
        for function in &module.functions {
            let Some((resource, callable)) =
                function_owned_value_resource(manifest, function, symbols)
            else {
                continue;
            };
            let resource_symbol = symbols
                .get(resource.cpp_symbol.as_str())
                .ok_or_else(|| format!("unknown C++ type symbol `{}`", resource.cpp_symbol))?;
            let mappings = public_cpp_parameter_mappings(function, callable);
            let parameter_count = mapped_cpp_parameter_count(function, callable);
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
                        Some(cpp_default_argument_expression(parameter))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let (selection, invocation) = if callable.overload_candidates > 1 {
                selected_overload_invocation(callable, &arguments)?
            } else if callable.kind == super::CppSymbolKind::Method {
                (
                    String::new(),
                    format!("value.{}({arguments})", callable.cpp_name),
                )
            } else {
                let callable_namespace = callable
                    .overload_set
                    .rsplit_once("::")
                    .map(|(namespace, _)| namespace);
                let call_name =
                    if callable_namespace == Some(manifest.cpp_metadata.namespace.as_str()) {
                        callable.cpp_name.as_str()
                    } else {
                        callable.overload_set.as_str()
                    };
                (String::new(), format!("{call_name}({arguments})"))
            };
            let result = if callable
                .returns
                .as_ref()
                .and_then(owned_unique_ptr_name)
                .is_some_and(|name| cpp_name_matches(name, &resource_symbol.cpp_name))
            {
                invocation
            } else {
                format!(
                    "std::make_unique<{}>({invocation})",
                    resource_symbol.overload_set
                )
            };
            source.push_str(&format!(
                "std::unique_ptr<{}> {}({}) noexcept {{\n  try {{\n{}    return {};\n  }} catch (...) {{\n    return nullptr;\n  }}\n}}\n\n",
                resource_symbol.overload_set,
                owned_value_adapter_name(module, function),
                cpp_parameters(manifest, function, callable)?,
                selection,
                result,
            ));
        }
    }
    for module in &manifest.modules {
        for function in &module.functions {
            let Some((resources, callable)) =
                function_owned_value_resource_tuple(manifest, function, symbols)
            else {
                continue;
            };
            let mappings = public_cpp_parameter_mappings(function, callable);
            let parameter_count = mapped_cpp_parameter_count(function, callable);
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
                        Some(cpp_default_argument_expression(parameter))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let (selection, invocation) = if callable.overload_candidates > 1 {
                selected_overload_invocation(callable, &arguments)?
            } else if callable.kind == super::CppSymbolKind::Method {
                (
                    String::new(),
                    format!("value.{}({arguments})", callable.cpp_name),
                )
            } else {
                (
                    String::new(),
                    format!("{}({arguments})", callable.overload_set),
                )
            };
            let carrier = owned_tuple_carrier_name(module, function);
            let owned_elements = resources
                .iter()
                .enumerate()
                .map(|(index, (_, _, symbol))| {
                    format!(
                        "std::make_unique<{}>(std::get<{index}>(std::move(result)))",
                        symbol.overload_set
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            source.push_str(&format!(
                "std::unique_ptr<{carrier}> {}({}) noexcept {{\n  try {{\n{}    auto result = {};\n    return std::make_unique<{carrier}>({owned_elements});\n  }} catch (...) {{\n    return nullptr;\n  }}\n}}\n",
                owned_tuple_adapter_name(module, function),
                cpp_parameters(manifest, function, callable)?,
                selection,
                invocation,
            ));
            for (index, (_, _, symbol)) in resources.iter().enumerate() {
                source.push_str(&format!(
                    "std::unique_ptr<{}> {}({carrier}& value) noexcept {{ return value.take_{index}(); }}\n",
                    symbol.overload_set,
                    owned_tuple_take_name(module, function, index)
                ));
            }
            source.push('\n');
        }
    }
    source.push_str(&format!(
        "}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

#[path = "owned_value_adapter/call_arguments.rs"]
mod call_arguments;
pub(super) use call_arguments::{
    cpp_call_argument_with_presence, cpp_default_argument_expression, cpp_parameter,
    cpp_parameters, owned_value_bridge_parameter_type, resource_list_input_resource,
    selected_overload_invocation, selected_overload_invocation_with_receiver,
    used_mutable_secondary_resources, used_resource_list_inputs,
};
use call_arguments::{snake_ident, upper_camel};
