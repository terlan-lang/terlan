//! Generated ownership adapters for opaque C++ resources returned by value.

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

/// Resolves a flat structural Terlan tuple whose elements are all opaque
/// package resources and proves that the selected callable returns the same
/// ordered `std::tuple` of C++ values.
pub(super) fn function_owned_value_resource_tuple<'a>(
    manifest: &'a NativeBindingManifest,
    function: &NativeBindingFunction,
    symbols: &'a BTreeMap<&str, &'a CppSymbol>,
) -> Option<(
    Vec<(
        &'a NativeBindingModule,
        &'a NativeBindingType,
        &'a CppSymbol,
    )>,
    &'a CppSymbol,
)> {
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
) -> Result<String, String> {
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
) -> Result<String, String> {
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

/// Converts Clang's declaration-oriented default spelling into a call-site
/// expression. Some generated ATen headers spell braced defaults as `={}`;
/// the leading declaration marker is not part of the expression itself.
pub(super) fn cpp_default_argument_expression(parameter: &CppParameter) -> String {
    let default = parameter
        .default
        .as_deref()
        .expect("validated omitted trailing C++ default")
        .trim();
    default
        .strip_prefix('=')
        .unwrap_or(default)
        .trim()
        .to_string()
}

/// Selects one exact Clang-extracted overload before invoking it. This avoids
/// relying on C++ conversion ranking and also permits the public API to omit
/// reviewed trailing defaults without leaving an ambiguous call expression.
pub(super) fn selected_overload_invocation(
    symbol: &CppSymbol,
    arguments: &str,
) -> Result<(String, String), String> {
    selected_overload_invocation_with_receiver(symbol, arguments, "value")
}

/// Selects an overload while allowing generated adapters to choose a receiver
/// identifier that cannot collide with an ordinary method parameter.
pub(super) fn selected_overload_invocation_with_receiver(
    symbol: &CppSymbol,
    arguments: &str,
    receiver_name: &str,
) -> Result<(String, String), String> {
    let returns = symbol
        .returns
        .as_ref()
        .ok_or_else(|| format!("overloaded C++ callable `{}` has no return type", symbol.id))?;
    let parameter_types = symbol
        .parameters
        .iter()
        .map(|parameter| parameter.ty.spelling.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if symbol.kind == super::CppSymbolKind::Method {
        let receiver = symbol
            .receiver
            .as_deref()
            .ok_or_else(|| format!("C++ method `{}` has no receiver", symbol.id))?;
        let qualifier = if symbol.receiver_mutable {
            ""
        } else {
            " const"
        };
        let selection = format!(
            "    using TerlanSelectedOverload = {} ({}::*)({}){};\n    const auto terlan_selected_overload = static_cast<TerlanSelectedOverload>(&{});\n",
            returns.spelling,
            receiver,
            parameter_types,
            qualifier,
            symbol.overload_set
        );
        Ok((
            selection,
            format!("({receiver_name}.*terlan_selected_overload)({arguments})"),
        ))
    } else {
        let selection = format!(
            "    using TerlanSelectedOverload = {} (*)({});\n    const auto terlan_selected_overload = static_cast<TerlanSelectedOverload>(&{});\n",
            returns.spelling, parameter_types, symbol.overload_set
        );
        Ok((selection, format!("terlan_selected_overload({arguments})")))
    }
}

/// Preserves exact extracted parameter spellings in generated C++ declarations.
pub(super) fn cpp_parameters(
    manifest: &NativeBindingManifest,
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
) -> Result<String, String> {
    let parameter_count = mapped_cpp_parameter_count(function, symbol);
    if parameter_count > symbol.parameters.len() {
        return Err(format!(
            "function `{}` maps more public values than extracted C++ parameters",
            function.name
        ));
    }
    let mut parameters = Vec::new();
    if symbol.kind == super::CppSymbolKind::Method {
        let receiver = symbol
            .receiver
            .as_deref()
            .ok_or_else(|| format!("C++ method `{}` has no receiver", symbol.id))?;
        parameters.push(format!("const {receiver}& value"));
    }
    let mappings = public_cpp_parameter_mappings(function, symbol);
    for mapping in mappings {
        let parameter = &symbol.parameters[mapping.cpp_parameter_index];
        if let Some(choice) = mapping.scalar_choice {
            parameters.push(format!("bool {}", choice.tag_name));
            parameters.push(format!("std::int64_t {}", choice.integer_name));
            parameters.push(format!("double {}", choice.floating_name));
            continue;
        }
        if let Some(presence) = mapping.presence_name {
            parameters.push(format!("bool {presence}"));
        }
        parameters.push(cpp_parameter(manifest, parameter, mapping.public_type));
    }
    Ok(parameters.join(", "))
}

/// Lowers one bridge-safe input and conditionally constructs a C++ optional
/// when the public API uses a `has_<name>, <name>` pair.
pub(super) fn cpp_call_argument_with_presence(
    manifest: &NativeBindingManifest,
    name: &str,
    parameter: &CppParameter,
    public_type: Option<&str>,
    presence_name: Option<&str>,
    scalar_choice: Option<&PublicScalarChoice<'_>>,
) -> String {
    if let Some(choice) = scalar_choice {
        let scalar = format!(
            "{} ? c10::Scalar({}) : c10::Scalar({})",
            choice.tag_name, choice.integer_name, choice.floating_name
        );
        return if is_optional_cpp_scalar_type(&parameter.ty) {
            format!("std::optional<c10::Scalar>({scalar})")
        } else {
            scalar
        };
    }
    let value = if resource_list_input_resource(manifest, public_type, &parameter.ty).is_some() {
        format!("{name}.values()")
    } else if let Some((_, resource_symbol)) =
        optional_resource_input_resource(manifest, public_type, &parameter.ty)
    {
        format!("std::optional<{}>({name})", resource_symbol.overload_set)
    } else if is_i64_array_ref_type(&parameter.ty) {
        format!("{}({name}.data(), {name}.size())", parameter.ty.spelling)
    } else if is_optional_i64_array_ref_type(&parameter.ty) {
        format!(
            "{}(c10::ArrayRef<std::int64_t>({name}.data(), {name}.size()))",
            parameter.ty.spelling
        )
    } else if is_cpp_scalar_type(&parameter.ty) && matches!(public_type, Some("Int" | "Float")) {
        format!("c10::Scalar({name})")
    } else if is_optional_cpp_scalar_type(&parameter.ty)
        && matches!(public_type, Some("Int" | "Float"))
    {
        format!("std::optional<c10::Scalar>(c10::Scalar({name}))")
    } else if matches!(public_type, Some("String"))
        && (is_rust_str_type(&parameter.ty) || is_cpp_string_view_type(&parameter.ty))
    {
        format!("{}({name}.data(), {name}.size())", parameter.ty.spelling)
    } else if matches!(public_type, Some("String"))
        && is_optional_cpp_string_view_type(&parameter.ty)
    {
        let inner = optional_cpp_inner_type(&parameter.ty)
            .expect("optional string-view recognition requires an inner type");
        format!(
            "{}({inner}({name}.data(), {name}.size()))",
            parameter.ty.spelling
        )
    } else if let Some(enum_name) = integer_enum_cpp_name(manifest, &parameter.ty, public_type) {
        format!("static_cast<{enum_name}>({name})")
    } else if let Some(enum_name) = optional_enum_cpp_name(manifest, &parameter.ty, public_type)
        .or_else(|| optional_integer_enum_cpp_name(manifest, &parameter.ty, public_type))
    {
        format!(
            "{}(static_cast<{enum_name}>({name}))",
            parameter.ty.spelling
        )
    } else if optional_primitive_bridge_type(&parameter.ty, public_type).is_some() {
        format!("{}({name})", parameter.ty.spelling)
    } else if let Some(value_name) =
        optional_string_value_cpp_name(manifest, &parameter.ty, public_type)
    {
        format!(
            "{}({value_name}(std::string({name}.data(), {name}.size())))",
            parameter.ty.spelling
        )
    } else {
        name.to_string()
    };
    presence_name
        .map(|presence| format!("{presence} ? {value} : std::nullopt"))
        .unwrap_or(value)
}

/// Renders the CXX-facing spelling of one generated adapter parameter.
pub(super) fn cpp_parameter(
    manifest: &NativeBindingManifest,
    parameter: &CppParameter,
    public_type: Option<&str>,
) -> String {
    if let Some((module, resource, _)) =
        resource_list_input_resource(manifest, public_type, &parameter.ty)
    {
        return format!(
            "const {}& {}",
            resource_list_input_name(module, resource),
            parameter.name
        );
    }
    if let Some((_, resource_symbol)) =
        optional_resource_input_resource(manifest, public_type, &parameter.ty)
    {
        return format!("const {}& {}", resource_symbol.overload_set, parameter.name);
    }
    if is_i64_array_ref_type(&parameter.ty) || is_optional_i64_array_ref_type(&parameter.ty) {
        return format!("rust::Slice<const std::int64_t> {}", parameter.name);
    }
    if is_cpp_scalar_type(&parameter.ty) || is_optional_cpp_scalar_type(&parameter.ty) {
        return match public_type {
            Some("Int") => format!("std::int64_t {}", parameter.name),
            Some("Float") => format!("double {}", parameter.name),
            _ => format!("{} {}", parameter.ty.spelling, parameter.name),
        };
    }
    if matches!(public_type, Some("String"))
        && (is_rust_str_type(&parameter.ty)
            || is_cpp_string_view_type(&parameter.ty)
            || is_optional_cpp_string_view_type(&parameter.ty))
    {
        return format!("rust::Str {}", parameter.name);
    }
    if integer_enum_cpp_name(manifest, &parameter.ty, public_type).is_some() {
        return format!("std::int64_t {}", parameter.name);
    }
    if optional_enum_cpp_name(manifest, &parameter.ty, public_type).is_some()
        || optional_integer_enum_cpp_name(manifest, &parameter.ty, public_type).is_some()
    {
        return format!("std::int64_t {}", parameter.name);
    }
    if let Some(bridge_type) = optional_primitive_cpp_type(&parameter.ty, public_type) {
        return format!("{bridge_type} {}", parameter.name);
    }
    if optional_string_value_cpp_name(manifest, &parameter.ty, public_type).is_some() {
        return format!("rust::Str {}", parameter.name);
    }
    format!("{} {}", parameter.ty.spelling, parameter.name)
}

/// Renders the Rust bridge type for one generated adapter parameter.
pub(super) fn owned_value_bridge_parameter_type(
    manifest: &NativeBindingManifest,
    parameter: &CppParameter,
    public_type: Option<&str>,
) -> Result<String, String> {
    if let Some((module, resource, _)) =
        resource_list_input_resource(manifest, public_type, &parameter.ty)
    {
        return Ok(format!("&{}", resource_list_input_name(module, resource)));
    }
    if let Some((_, resource_symbol)) =
        optional_resource_input_resource(manifest, public_type, &parameter.ty)
    {
        return Ok(format!("&{}", resource_symbol.cpp_name));
    }
    if is_i64_array_ref_type(&parameter.ty) || is_optional_i64_array_ref_type(&parameter.ty) {
        return Ok("&[i64]".into());
    }
    if is_cpp_scalar_type(&parameter.ty) || is_optional_cpp_scalar_type(&parameter.ty) {
        return match public_type {
            Some("Int") => Ok("i64".into()),
            Some("Float") => Ok("f64".into()),
            _ => rust_bridge_type(&parameter.ty),
        };
    }
    if matches!(public_type, Some("String"))
        && (is_rust_str_type(&parameter.ty)
            || is_cpp_string_view_type(&parameter.ty)
            || is_optional_cpp_string_view_type(&parameter.ty))
    {
        return Ok("&str".into());
    }
    if integer_enum_cpp_name(manifest, &parameter.ty, public_type).is_some() {
        return Ok("i64".into());
    }
    if optional_enum_cpp_name(manifest, &parameter.ty, public_type).is_some()
        || optional_integer_enum_cpp_name(manifest, &parameter.ty, public_type).is_some()
    {
        return Ok("i64".into());
    }
    if let Some(bridge_type) = optional_primitive_bridge_type(&parameter.ty, public_type) {
        return Ok(bridge_type.into());
    }
    if optional_string_value_cpp_name(manifest, &parameter.ty, public_type).is_some() {
        return Ok("&str".into());
    }
    rust_bridge_type(&parameter.ty)
}

/// Resolves a public resource list and its exact extracted C++ element type.
pub(super) fn resource_list_input_resource<'a>(
    manifest: &'a NativeBindingManifest,
    public_type: Option<&str>,
    cpp_type: &CppTypeMetadata,
) -> Option<(
    &'a NativeBindingModule,
    &'a NativeBindingType,
    &'a CppSymbol,
)> {
    let public_element = public_type?
        .strip_prefix("List[")?
        .strip_suffix(']')?
        .trim();
    let cpp_element = cpp_array_ref_element(cpp_type)?;
    manifest.modules.iter().find_map(|module| {
        module.types.iter().find_map(|resource| {
            if resource.kind != NativeBindingTypeKind::OpaqueResource
                || !terlan_type_matches(public_element, &resource.name)
            {
                return None;
            }
            let symbol = manifest
                .cpp_metadata
                .symbols
                .iter()
                .find(|symbol| symbol.id == resource.cpp_symbol)?;
            (cpp_name_matches(cpp_element, &symbol.cpp_name)
                || cpp_name_matches(cpp_element, &symbol.overload_set))
            .then_some((module, resource, symbol))
        })
    })
}

/// Resolves a public opaque resource supplied as a present
/// `std::optional<Resource>` value.
pub(super) fn optional_resource_input_resource<'a>(
    manifest: &'a NativeBindingManifest,
    public_type: Option<&str>,
    cpp_type: &CppTypeMetadata,
) -> Option<(&'a NativeBindingType, &'a CppSymbol)> {
    let public_type = public_type?;
    let inner = optional_cpp_inner_type(cpp_type)?;
    manifest.modules.iter().find_map(|module| {
        module.types.iter().find_map(|resource| {
            if resource.kind != NativeBindingTypeKind::OpaqueResource
                || !terlan_type_matches(public_type, &resource.name)
            {
                return None;
            }
            let symbol = manifest
                .cpp_metadata
                .symbols
                .iter()
                .find(|symbol| symbol.id == resource.cpp_symbol)?;
            (cpp_name_matches(inner, &symbol.cpp_name)
                || cpp_name_matches(inner, &symbol.overload_set))
            .then_some((resource, symbol))
        })
    })
}

/// Collects the distinct resource-list input collectors required by generated
/// adapters. One collector is shared by every callable using the same resource.
pub(super) fn used_resource_list_inputs<'a>(
    manifest: &'a NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Vec<(
    &'a NativeBindingModule,
    &'a NativeBindingType,
    &'a CppSymbol,
)> {
    let mut inputs = BTreeMap::new();
    for module in &manifest.modules {
        for function in &module.functions {
            if function_owned_value_resource(manifest, function, symbols).is_none()
                && !matches!(
                    function.role,
                    NativeFunctionRole::ResourceListProjection
                        | NativeFunctionRole::MutableFreeFunction
                )
            {
                continue;
            }
            let Ok(callable) = function_symbol(function, symbols) else {
                continue;
            };
            for mapping in public_cpp_parameter_mappings(function, callable) {
                let parameter = &callable.parameters[mapping.cpp_parameter_index];
                let Some((owner, resource, symbol)) =
                    resource_list_input_resource(manifest, mapping.public_type, &parameter.ty)
                else {
                    continue;
                };
                inputs
                    .entry(resource_list_input_name(owner, resource))
                    .or_insert((owner, resource, symbol));
            }
        }
    }
    inputs.into_values().collect()
}

/// Collects resource values that must be copied before a mutable handle-table
/// borrow. Copying preserves valid aliasing when the same handle is also the
/// mutation target and keeps Rust borrowing entirely safe.
pub(super) fn used_mutable_secondary_resources<'a>(
    manifest: &'a NativeBindingManifest,
    symbols: &BTreeMap<&str, &'a CppSymbol>,
) -> Vec<(
    &'a NativeBindingModule,
    &'a NativeBindingType,
    &'a CppSymbol,
)> {
    let mut inputs = BTreeMap::new();
    for function in manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| {
            function.role == NativeFunctionRole::MutableFreeFunction
                || function.role == NativeFunctionRole::MutableMethod
                    && function.cpp_symbol.as_deref().is_some_and(|symbol| {
                        manifest
                            .mapping
                            .symbols
                            .iter()
                            .find(|policy| policy.symbol == symbol)
                            .is_some_and(|policy| policy.exception.is_some())
                    })
        })
    {
        for argument in function.args.iter().skip(1) {
            if argument.mutable {
                continue;
            }
            let Some((owner, resource)) = manifest.modules.iter().find_map(|module| {
                module.types.iter().find_map(|resource| {
                    (resource.kind == NativeBindingTypeKind::OpaqueResource
                        && terlan_type_matches(&argument.ty, &resource.name))
                    .then_some((module, resource))
                })
            }) else {
                continue;
            };
            let Some(symbol) = symbols.get(resource.cpp_symbol.as_str()).copied() else {
                continue;
            };
            inputs
                .entry(resource_input_copy_name(owner, resource))
                .or_insert((owner, resource, symbol));
        }
    }
    inputs.into_values().collect()
}

/// Resolves a reviewed public enum supplied to `std::optional<CppEnum>`.
fn optional_enum_cpp_name<'a>(
    manifest: &'a NativeBindingManifest,
    cpp_type: &CppTypeMetadata,
    public_type: Option<&str>,
) -> Option<&'a str> {
    let public_type = public_type?;
    let inner = optional_cpp_inner_type(cpp_type)?;
    let binding_type = manifest
        .modules
        .iter()
        .flat_map(|module| &module.types)
        .find(|ty| {
            ty.kind == NativeBindingTypeKind::Enum && terlan_type_matches(public_type, &ty.name)
        })?;
    let symbol = manifest
        .cpp_metadata
        .symbols
        .iter()
        .find(|symbol| symbol.id == binding_type.cpp_symbol)?;
    (cpp_name_matches(inner, &symbol.cpp_name) || cpp_name_matches(inner, &symbol.overload_set))
        .then_some(symbol.overload_set.as_str())
}

/// Resolves an exact extracted C++ enum when a compatibility API intentionally
/// retains its historical integer representation.
fn integer_enum_cpp_name<'a>(
    manifest: &'a NativeBindingManifest,
    cpp_type: &CppTypeMetadata,
    public_type: Option<&str>,
) -> Option<&'a str> {
    if public_type != Some("Int") || !cpp_type.enum_type {
        return None;
    }
    manifest
        .cpp_metadata
        .symbols
        .iter()
        .find(|symbol| {
            symbol.kind == super::CppSymbolKind::Enum
                && (cpp_name_matches(&cpp_type.canonical, &symbol.cpp_name)
                    || cpp_name_matches(&cpp_type.canonical, &symbol.overload_set))
        })
        .map(|symbol| symbol.overload_set.as_str())
}

/// Resolves an exact extracted optional C++ enum when a compatibility API
/// intentionally retains its historical integer representation.
fn optional_integer_enum_cpp_name<'a>(
    manifest: &'a NativeBindingManifest,
    cpp_type: &CppTypeMetadata,
    public_type: Option<&str>,
) -> Option<&'a str> {
    if public_type != Some("Int") {
        return None;
    }
    let inner = optional_cpp_inner_type(cpp_type)?;
    manifest
        .cpp_metadata
        .symbols
        .iter()
        .find(|symbol| {
            symbol.kind == super::CppSymbolKind::Enum
                && (cpp_name_matches(inner, &symbol.cpp_name)
                    || cpp_name_matches(inner, &symbol.overload_set))
        })
        .map(|symbol| symbol.overload_set.as_str())
}

/// Resolves a reviewed transparent String supplied to
/// `std::optional<CppValue>` through direct value construction.
fn optional_string_value_cpp_name<'a>(
    manifest: &'a NativeBindingManifest,
    cpp_type: &CppTypeMetadata,
    public_type: Option<&str>,
) -> Option<&'a str> {
    let public_type = public_type?;
    let inner = optional_cpp_inner_type(cpp_type)?;
    let binding_type = manifest
        .modules
        .iter()
        .flat_map(|module| &module.types)
        .find(|ty| {
            ty.kind == NativeBindingTypeKind::StringValue
                && terlan_type_matches(public_type, &ty.name)
        })?;
    let symbol = manifest
        .cpp_metadata
        .symbols
        .iter()
        .find(|symbol| symbol.id == binding_type.cpp_symbol)?;
    (cpp_name_matches(inner, &symbol.cpp_name) || cpp_name_matches(inner, &symbol.overload_set))
        .then_some(symbol.overload_set.as_str())
}

/// C++ bridge scalar used for a primitive `std::optional<T>` input.
fn optional_primitive_cpp_type(
    cpp_type: &CppTypeMetadata,
    public_type: Option<&str>,
) -> Option<&'static str> {
    let inner = optional_cpp_inner_type(cpp_type)?;
    match public_type? {
        "Int" if matches!(inner, "long" | "long long" | "std::int64_t" | "int64_t") => {
            Some("std::int64_t")
        }
        "Float" if inner == "double" => Some("double"),
        "Bool" if inner == "bool" => Some("bool"),
        _ => None,
    }
}

/// Rust bridge scalar used for a primitive `std::optional<T>` input.
fn optional_primitive_bridge_type(
    cpp_type: &CppTypeMetadata,
    public_type: Option<&str>,
) -> Option<&'static str> {
    match optional_primitive_cpp_type(cpp_type, public_type)? {
        "std::int64_t" => Some("i64"),
        "double" => Some("f64"),
        "bool" => Some("bool"),
        _ => None,
    }
}

fn snake_ident(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn upper_camel(value: &str) -> String {
    let mut output = String::new();
    let mut uppercase = true;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if uppercase {
                output.push(character.to_ascii_uppercase());
                uppercase = false;
            } else {
                output.push(character);
            }
        } else {
            uppercase = true;
        }
    }
    output
}
