//! Ownership-aware C++ call arguments and overload selection.

use super::*;
use crate::commands::bind::cpp_binding_generator::error::CppBindingError;

/// Converts Clang's declaration-oriented default spelling into a call-site
/// expression. Some generated ATen headers spell braced defaults as `={}`;
/// the leading declaration marker is not part of the expression itself.
pub(in crate::commands::bind::cpp_binding_generator::generator) fn cpp_default_argument_expression(
    parameter: &CppParameter,
) -> String {
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
pub(in crate::commands::bind::cpp_binding_generator::generator) fn selected_overload_invocation(
    symbol: &CppSymbol,
    arguments: &str,
) -> Result<(String, String), CppBindingError> {
    selected_overload_invocation_with_receiver(symbol, arguments, "value")
}

/// Selects an overload while allowing generated adapters to choose a receiver
/// identifier that cannot collide with an ordinary method parameter.
pub(in crate::commands::bind::cpp_binding_generator::generator) fn selected_overload_invocation_with_receiver(
    symbol: &CppSymbol,
    arguments: &str,
    receiver_name: &str,
) -> Result<(String, String), CppBindingError> {
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
    if symbol.kind == super::super::CppSymbolKind::Method {
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
pub(in crate::commands::bind::cpp_binding_generator::generator) fn cpp_parameters(
    manifest: &NativeBindingManifest,
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
) -> Result<String, CppBindingError> {
    let parameter_count = mapped_cpp_parameter_count(function, symbol);
    if parameter_count > symbol.parameters.len() {
        return Err((format!(
            "function `{}` maps more public values than extracted C++ parameters",
            function.name
        ))
        .into());
    }
    let mut parameters = Vec::new();
    if symbol.kind == super::super::CppSymbolKind::Method {
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
pub(in crate::commands::bind::cpp_binding_generator::generator) fn cpp_call_argument_with_presence(
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
pub(in crate::commands::bind::cpp_binding_generator::generator) fn cpp_parameter(
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
pub(in crate::commands::bind::cpp_binding_generator::generator) fn owned_value_bridge_parameter_type(
    manifest: &NativeBindingManifest,
    parameter: &CppParameter,
    public_type: Option<&str>,
) -> Result<String, CppBindingError> {
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
pub(in crate::commands::bind::cpp_binding_generator::generator) fn resource_list_input_resource<
    'a,
>(
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
pub(in crate::commands::bind::cpp_binding_generator::generator) fn optional_resource_input_resource<
    'a,
>(
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
pub(in crate::commands::bind::cpp_binding_generator::generator) fn used_resource_list_inputs<'a>(
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
pub(in crate::commands::bind::cpp_binding_generator::generator) fn used_mutable_secondary_resources<
    'a,
>(
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
pub(super) fn optional_enum_cpp_name<'a>(
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
pub(super) fn integer_enum_cpp_name<'a>(
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
            symbol.kind == super::super::CppSymbolKind::Enum
                && (cpp_name_matches(&cpp_type.canonical, &symbol.cpp_name)
                    || cpp_name_matches(&cpp_type.canonical, &symbol.overload_set))
        })
        .map(|symbol| symbol.overload_set.as_str())
}

/// Resolves an exact extracted optional C++ enum when a compatibility API
/// intentionally retains its historical integer representation.
pub(super) fn optional_integer_enum_cpp_name<'a>(
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
            symbol.kind == super::super::CppSymbolKind::Enum
                && (cpp_name_matches(inner, &symbol.cpp_name)
                    || cpp_name_matches(inner, &symbol.overload_set))
        })
        .map(|symbol| symbol.overload_set.as_str())
}

/// Resolves a reviewed transparent String supplied to
/// `std::optional<CppValue>` through direct value construction.
pub(super) fn optional_string_value_cpp_name<'a>(
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
pub(super) fn optional_primitive_cpp_type(
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
pub(super) fn optional_primitive_bridge_type(
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

pub(super) fn snake_ident(value: &str) -> String {
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

pub(super) fn upper_camel(value: &str) -> String {
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
