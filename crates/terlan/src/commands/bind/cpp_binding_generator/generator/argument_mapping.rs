use super::*;

/// Requires every public argument to match its extracted C++ parameter shape.
pub(super) fn validate_function_argument_mapping(
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), CppBindingError> {
    let contained_mutable_method = function.role == NativeFunctionRole::MutableMethod
        && symbols
            .policies
            .get(symbol.id.as_str())
            .is_some_and(|policy| policy.exception.is_some());
    if function.role == NativeFunctionRole::MutableMethod
        && !contained_mutable_method
        && function.args.iter().skip(1).any(|argument| {
            find_resource_type_in_modules(modules, &argument.ty).is_some()
                || argument
                    .ty
                    .strip_prefix("List[")
                    .and_then(|value| value.strip_suffix(']'))
                    .is_some_and(|element| {
                        find_resource_type_in_modules(modules, element.trim()).is_some()
                    })
        })
    {
        return Err((format!(
            "error[cpp.lifetime.mutable_alias]: mutable method `{}` cannot borrow a second opaque resource",
            function.name
        )).into());
    }
    let args = if symbol.kind == CppSymbolKind::Method {
        function.args.get(1..).unwrap_or_default()
    } else {
        function.args.as_slice()
    };
    if args
        .iter()
        .any(|argument| argument.cpp_parameter.is_some() || argument.cpp_ignore)
    {
        return validate_explicit_function_argument_mapping(
            function, symbol, args, modules, symbols,
        );
    }
    let mut argument_index = 0;
    let mut parameter_index = 0;
    while argument_index < args.len() {
        let argument = &args[argument_index];
        if argument.prepend_resource {
            return Err((format!(
                "error[cpp.type.resource_list_prepend]: function `{}` argument `{}` must immediately follow the opaque resource copied into its resource list",
                function.name, argument.name
            )).into());
        }
        if args
            .get(argument_index + 1)
            .is_some_and(|next| next.prepend_resource)
        {
            let list = &args[argument_index + 1];
            let resource = find_resource_type_in_modules(modules, &argument.ty);
            let list_resource = list
                .ty
                .strip_prefix("List[")
                .and_then(|value| value.strip_suffix(']'))
                .map(str::trim)
                .and_then(|value| find_resource_type_in_modules(modules, value));
            if resource.is_none()
                || resource.map(|ty| ty.name.as_str()) != list_resource.map(|ty| ty.name.as_str())
                || !argument.fields.is_empty()
                || argument.default.is_some()
                || argument.mutable
                || !list.fields.is_empty()
                || list.default.is_some()
                || list.mutable
            {
                return Err((format!(
                    "error[cpp.type.resource_list_prepend]: function `{}` arguments `{}` and `{}` must be an immutable opaque resource followed by a matching immutable resource list without defaults or field projections",
                    function.name, argument.name, list.name
                )).into());
            }
            let parameter = symbol.parameters.get(parameter_index).ok_or_else(|| {
                format!(
                    "error[cpp.type.argument_count]: function `{}` maps more public values than the {} extracted C++ parameters",
                    function.name,
                    symbol.parameters.len()
                )
            })?;
            validate_scalar_argument(function, list, parameter, modules, symbols)?;
            argument_index += 2;
            parameter_index += 1;
            continue;
        }
        if argument.fields.is_empty()
            && should_omit_default_optional_parameter(
                args,
                argument_index,
                &symbol.parameters,
                parameter_index,
            )
        {
            parameter_index += 1;
            continue;
        }
        if let Some(parameter) = symbol.parameters.get(parameter_index) {
            if scalar_choice_triple(args, argument_index, parameter) {
                argument_index += 3;
                parameter_index += 1;
                continue;
            }
            if optional_presence_pair(args, argument_index, parameter) {
                let value = &args[argument_index + 1];
                validate_scalar_argument(function, value, parameter, modules, symbols)?;
                argument_index += 2;
                parameter_index += 1;
                continue;
            }
        }
        if argument.fields.is_empty() {
            let parameter = symbol.parameters.get(parameter_index).ok_or_else(|| {
                format!(
                    "error[cpp.type.argument_count]: function `{}` maps more public values than the {} extracted C++ parameters",
                    function.name,
                    symbol.parameters.len()
                )
            })?;
            validate_scalar_argument(function, argument, parameter, modules, symbols)?;
            argument_index += 1;
            parameter_index += 1;
            continue;
        }

        let record = modules
            .iter()
            .flat_map(|module| &module.types)
            .find(|ty| {
                ty.kind == NativeBindingTypeKind::ValueRecord
                    && terlan_type_matches(&argument.ty, &ty.name)
            })
            .ok_or_else(|| {
                format!(
                    "error[cpp.type.record_argument]: function `{}` argument `{}` declares field projections but `{}` is not a copied value record",
                    function.name, argument.name, argument.ty
                )
            })?;
        if argument.fields.len() != record.fields.len() {
            return Err((format!(
                "error[cpp.type.record_argument_fields]: function `{}` argument `{}` must map all {} fields from `{}`",
                function.name,
                argument.name,
                record.fields.len(),
                record.name
            )).into());
        }
        let mut public_fields = BTreeSet::new();
        let mut cpp_parameters = BTreeSet::new();
        for mapping in &argument.fields {
            if !public_fields.insert(mapping.field.as_str()) {
                return Err((format!(
                    "error[cpp.type.record_argument_duplicate]: function `{}` argument `{}` maps field `{}` more than once",
                    function.name, argument.name, mapping.field
                )).into());
            }
            if !cpp_parameters.insert(mapping.cpp_parameter.as_str()) {
                return Err((format!(
                    "error[cpp.type.record_parameter_duplicate]: function `{}` argument `{}` maps C++ parameter `{}` more than once",
                    function.name, argument.name, mapping.cpp_parameter
                )).into());
            }
            let field = record
                .fields
                .iter()
                .find(|field| field.name == mapping.field)
                .ok_or_else(|| {
                    format!(
                        "error[cpp.type.record_argument_field]: function `{}` argument `{}` references unknown field `{}`",
                        function.name, argument.name, mapping.field
                    )
                })?;
            let parameter = symbol.parameters.get(parameter_index).ok_or_else(|| {
                format!(
                    "error[cpp.type.argument_count]: function `{}` record argument `{}` exceeds the {} extracted C++ parameters",
                    function.name,
                    argument.name,
                    symbol.parameters.len()
                )
            })?;
            if parameter.name != mapping.cpp_parameter {
                return Err((format!(
                    "error[cpp.type.record_parameter_order]: function `{}` field `{}.{}` must map the next extracted C++ parameter `{}`, not `{}`",
                    function.name,
                    argument.name,
                    mapping.field,
                    parameter.name,
                    mapping.cpp_parameter
                )).into());
            }
            if !terlan_primitive_matches_cpp(&field.ty, &parameter.ty) {
                return Err((format!(
                    "error[cpp.type.record_argument_mapping_mismatch]: function `{}` field `{}.{}` maps incompatible Terlan type `{}` to C++ parameter `{}`",
                    function.name,
                    argument.name,
                    mapping.field,
                    field.ty,
                    parameter.ty.canonical
                )).into());
            }
            parameter_index += 1;
        }
        argument_index += 1;
    }
    if symbol.parameters[parameter_index..]
        .iter()
        .any(|parameter| {
            parameter.default.is_none() && !is_explicitly_omittable_optional(&parameter.ty)
        })
    {
        return Err((format!(
            "error[cpp.type.argument_count]: function `{}` maps {parameter_index} public scalar values but one of the remaining {} C++ parameters has no default",
            function.name,
            symbol.parameters.len() - parameter_index
        )).into());
    }
    Ok(())
}

/// Validates a declarative public-to-C++ argument permutation.
///
/// Every logical public value must name its extracted target when permutation
/// is enabled. Tagged Scalars and optional presence/value pairs name the target
/// on their first public field. Requiring a complete bijection keeps generated
/// adapter signatures in public order while exact upstream calls remain in C++
/// declaration order, without silently inventing defaults or conversions.
pub(super) fn validate_explicit_function_argument_mapping(
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
    args: &[NativeBindingArg],
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), CppBindingError> {
    let mut argument_index = 0;
    let mut mapped_parameters = BTreeSet::new();
    while argument_index < args.len() {
        let argument = &args[argument_index];
        if argument.cpp_ignore {
            let resource = find_resource_type_in_modules(modules, &argument.ty).is_some()
                || argument
                    .ty
                    .strip_prefix("List[")
                    .and_then(|value| value.strip_suffix(']'))
                    .is_some_and(|element| {
                        find_resource_type_in_modules(modules, element.trim()).is_some()
                    });
            let value_record = modules.iter().flat_map(|module| &module.types).any(|ty| {
                ty.kind == NativeBindingTypeKind::ValueRecord
                    && terlan_type_matches(&argument.ty, &ty.name)
            });
            if argument.cpp_parameter.is_some()
                || argument.prepend_resource
                || !argument.fields.is_empty()
                || argument.mutable
                || resource
                || value_record
            {
                return Err((format!(
                    "error[cpp.type.ignored_parameter_shape]: function `{}` argument `{}` marked `cpp_ignore` must be an immutable copied value without `cpp_parameter`, resource-list prepending, or record-field expansion",
                    function.name, argument.name
                )).into());
            }
            argument_index += 1;
            continue;
        }
        if argument.prepend_resource || !argument.fields.is_empty() {
            return Err((format!(
                "error[cpp.type.explicit_parameter_shape]: function `{}` argument `{}` cannot combine `cpp_parameter` permutation with resource-list prepending or record-field expansion",
                function.name, argument.name
            )).into());
        }
        let target = argument.cpp_parameter.as_deref().ok_or_else(|| {
            format!(
                "error[cpp.type.explicit_parameter_missing]: function `{}` argument `{}` must name its extracted `cpp_parameter` because this function permutes arguments",
                function.name, argument.name
            )
        })?;
        let parameter_index = symbol
            .parameters
            .iter()
            .position(|parameter| parameter.name == target)
            .ok_or_else(|| {
                format!(
                    "error[cpp.type.explicit_parameter_unknown]: function `{}` argument `{}` names unknown C++ parameter `{target}`",
                    function.name, argument.name
                )
            })?;
        if !mapped_parameters.insert(parameter_index) {
            return Err((format!(
                "error[cpp.type.explicit_parameter_duplicate]: function `{}` maps C++ parameter `{target}` more than once",
                function.name
            )).into());
        }
        let parameter = &symbol.parameters[parameter_index];
        if scalar_choice_triple(args, argument_index, parameter) {
            if args[argument_index + 1].cpp_parameter.is_some()
                || args[argument_index + 1].cpp_ignore
                || args[argument_index + 2].cpp_parameter.is_some()
                || args[argument_index + 2].cpp_ignore
            {
                return Err((format!(
                    "error[cpp.type.explicit_parameter_group]: function `{}` must name `cpp_parameter` only on the first field of tagged Scalar `{}`",
                    function.name, argument.name
                )).into());
            }
            argument_index += 3;
            continue;
        }
        if optional_presence_pair(args, argument_index, parameter) {
            if args[argument_index + 1].cpp_parameter.is_some()
                || args[argument_index + 1].cpp_ignore
            {
                return Err((format!(
                    "error[cpp.type.explicit_parameter_group]: function `{}` must name `cpp_parameter` only on optional presence flag `{}`",
                    function.name, argument.name
                )).into());
            }
            validate_scalar_argument(
                function,
                &args[argument_index + 1],
                parameter,
                modules,
                symbols,
            )?;
            argument_index += 2;
            continue;
        }
        validate_scalar_argument(function, argument, parameter, modules, symbols)?;
        argument_index += 1;
    }
    let missing = symbol
        .parameters
        .iter()
        .enumerate()
        .filter(|(index, parameter)| {
            !mapped_parameters.contains(index)
                && parameter.default.is_none()
                && !is_explicitly_omittable_optional(&parameter.ty)
        })
        .map(|(_, parameter)| parameter.name.as_str())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err((format!(
            "error[cpp.type.explicit_parameter_incomplete]: function `{}` does not map extracted C++ parameters {}",
            function.name,
            missing.join(", ")
        )).into());
    }
    Ok(())
}

/// Requires one ordinary public argument to match one extracted C++ parameter.
pub(super) fn validate_scalar_argument(
    function: &NativeBindingFunction,
    argument: &NativeBindingArg,
    parameter: &CppParameter,
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), CppBindingError> {
    let compatible = terlan_primitive_matches_cpp(&argument.ty, &parameter.ty)
        || optional_cpp_inner_type(&parameter.ty)
            .is_some_and(|inner| terlan_primitive_matches_optional_cpp(&argument.ty, inner))
        || matches!(argument.ty.as_str(), "Int" | "Float")
            && (is_cpp_scalar_type(&parameter.ty) || is_optional_cpp_scalar_type(&parameter.ty))
        || argument.ty == "Bool"
            && optional_cpp_inner_type(&parameter.ty).is_some_and(|inner| inner == "bool")
        || matches!(argument.ty.as_str(), "String")
            && (is_rust_str_type(&parameter.ty)
                || is_cpp_string_view_type(&parameter.ty)
                || is_optional_cpp_string_view_type(&parameter.ty))
        || matches!(argument.ty.as_str(), "std.vm.Bytes.Bytes" | "Bytes")
            && is_u8_slice_type(&parameter.ty)
        || argument.ty == "List[Int]"
            && (is_i64_slice_type(&parameter.ty)
                || is_i64_array_ref_type(&parameter.ty)
                || is_optional_i64_array_ref_type(&parameter.ty))
        || argument.ty == "List[Float]" && is_f64_slice_type(&parameter.ty)
        || resource_list_argument_resource(modules, symbols, &argument.ty, &parameter.ty).is_some()
        || modules
            .iter()
            .flat_map(|module| &module.types)
            .any(|binding_type| {
                binding_type.kind == NativeBindingTypeKind::Enum
                    && terlan_type_matches(&argument.ty, &binding_type.name)
                    && (is_i64_type(&parameter.ty)
                        || optional_cpp_inner_type(&parameter.ty).is_some_and(|inner| {
                            symbols
                                .declarations
                                .get(binding_type.cpp_symbol.as_str())
                                .is_some_and(|enum_symbol| {
                                    cpp_name_matches(inner, &enum_symbol.cpp_name)
                                        || cpp_name_matches(inner, &enum_symbol.overload_set)
                                })
                        }))
            })
        || modules
            .iter()
            .flat_map(|module| &module.types)
            .any(|binding_type| {
                binding_type.kind == NativeBindingTypeKind::StringValue
                    && terlan_type_matches(&argument.ty, &binding_type.name)
                    && optional_cpp_inner_type(&parameter.ty).is_some_and(|inner| {
                        symbols
                            .declarations
                            .get(binding_type.cpp_symbol.as_str())
                            .is_some_and(|record_symbol| {
                                cpp_name_matches(inner, &record_symbol.cpp_name)
                                    || cpp_name_matches(inner, &record_symbol.overload_set)
                            })
                    })
            })
        || modules
            .iter()
            .flat_map(|module| &module.types)
            .find(|binding_type| {
                binding_type.kind == NativeBindingTypeKind::OpaqueResource
                    && terlan_type_matches(&argument.ty, &binding_type.name)
            })
            .and_then(|binding_type| symbols.declarations.get(binding_type.cpp_symbol.as_str()))
            .is_some_and(|resource_symbol| {
                borrowed_const_record_name(&parameter.ty).is_some_and(|name| {
                    cpp_name_matches(name, &resource_symbol.cpp_name)
                        || cpp_name_matches(name, &resource_symbol.overload_set)
                }) || optional_cpp_inner_type(&parameter.ty).is_some_and(|name| {
                    cpp_name_matches(name, &resource_symbol.cpp_name)
                        || cpp_name_matches(name, &resource_symbol.overload_set)
                }) || function.role == NativeFunctionRole::MutableFreeFunction
                    && parameter.direction != CppParameterDirection::Input
                    && mutable_record_reference_name(&parameter.ty).is_some_and(|name| {
                        cpp_name_matches(name, &resource_symbol.cpp_name)
                            || cpp_name_matches(name, &resource_symbol.overload_set)
                    })
            })
        || argument.ty == "Int"
            && optional_cpp_inner_type(&parameter.ty).is_some_and(|inner| {
                symbols.declarations.values().any(|symbol| {
                    symbol.kind == CppSymbolKind::Enum
                        && (cpp_name_matches(inner, &symbol.cpp_name)
                            || cpp_name_matches(inner, &symbol.overload_set))
                })
            })
        || argument.ty == "Int"
            && parameter.ty.enum_type
            && symbols.declarations.values().any(|symbol| {
                symbol.kind == CppSymbolKind::Enum
                    && (cpp_name_matches(&parameter.ty.canonical, &symbol.cpp_name)
                        || cpp_name_matches(&parameter.ty.canonical, &symbol.overload_set))
            });
    if compatible {
        return Ok(());
    }
    Err((format!(
        "error[cpp.type.argument_mapping_mismatch]: function `{}` maps C++ parameter `{}` to incompatible Terlan type `{}`",
        function.name, parameter.ty.canonical, argument.ty
    )).into())
}

/// Resolves one opaque resource type without requiring a complete manifest.
pub(super) fn find_resource_type_in_modules<'a>(
    modules: &'a [NativeBindingModule],
    value: &str,
) -> Option<&'a NativeBindingType> {
    modules.iter().flat_map(|module| &module.types).find(|ty| {
        ty.kind == NativeBindingTypeKind::OpaqueResource && terlan_type_matches(value, &ty.name)
    })
}

/// Resolves a public `List[Resource]` supplied to a C++ `ArrayRef<Resource>`.
pub(super) fn resource_list_argument_resource<'a>(
    modules: &'a [NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
    public_type: &str,
    cpp_type: &CppTypeMetadata,
) -> Option<&'a NativeBindingType> {
    let public_element = public_type.strip_prefix("List[")?.strip_suffix(']')?.trim();
    let cpp_element = cpp_array_ref_element(cpp_type)?;
    modules.iter().flat_map(|module| &module.types).find(|ty| {
        ty.kind == NativeBindingTypeKind::OpaqueResource
            && terlan_type_matches(public_element, &ty.name)
            && symbols
                .declarations
                .get(ty.cpp_symbol.as_str())
                .is_some_and(|symbol| {
                    cpp_name_matches(cpp_element, &symbol.cpp_name)
                        || cpp_name_matches(cpp_element, &symbol.overload_set)
                })
    })
}

/// Matches copied Terlan scalar types to supported by-value C++ scalars.
pub(super) fn terlan_primitive_matches_cpp(ty: &str, cpp_type: &CppTypeMetadata) -> bool {
    match ty {
        "Int" => is_i64_type(cpp_type),
        "Float" => cpp_type.canonical == "double",
        "Bool" => cpp_type.canonical == "bool",
        _ => false,
    }
}

/// Aligns public arguments with extracted parameters, including copied-record
/// expansion and conventional optional presence/value pairs.
pub(super) fn public_cpp_parameter_mappings<'a>(
    function: &'a NativeBindingFunction,
    symbol: &'a CppSymbol,
) -> Vec<PublicCppParameterMapping<'a>> {
    let public_start = usize::from(symbol.kind == CppSymbolKind::Method);
    let args = if symbol.kind == CppSymbolKind::Method {
        function.args.get(1..).unwrap_or_default()
    } else {
        function.args.as_slice()
    };
    if args
        .iter()
        .any(|argument| argument.cpp_parameter.is_some() || argument.cpp_ignore)
    {
        return explicit_public_cpp_parameter_mappings(args, public_start, symbol);
    }
    let mut mappings = Vec::new();
    let mut argument_index = 0;
    let mut parameter_index = 0;
    while argument_index < args.len() && parameter_index < symbol.parameters.len() {
        let argument = &args[argument_index];
        if args
            .get(argument_index + 1)
            .is_some_and(|next| next.prepend_resource)
        {
            let list = &args[argument_index + 1];
            mappings.push(PublicCppParameterMapping {
                cpp_parameter_index: parameter_index,
                public_type: Some(list.ty.as_str()),
                presence_name: None,
                public_argument_index: public_start + argument_index + 1,
                scalar_choice: None,
            });
            argument_index += 2;
            parameter_index += 1;
            continue;
        }
        if argument.fields.is_empty()
            && should_omit_default_optional_parameter(
                args,
                argument_index,
                &symbol.parameters,
                parameter_index,
            )
        {
            parameter_index += 1;
            continue;
        }
        let parameter = &symbol.parameters[parameter_index];
        if scalar_choice_triple(args, argument_index, parameter) {
            mappings.push(PublicCppParameterMapping {
                cpp_parameter_index: parameter_index,
                public_type: None,
                presence_name: None,
                public_argument_index: public_start + argument_index,
                scalar_choice: Some(PublicScalarChoice {
                    tag_name: argument.name.as_str(),
                    integer_name: args[argument_index + 1].name.as_str(),
                    floating_name: args[argument_index + 2].name.as_str(),
                }),
            });
            argument_index += 3;
            parameter_index += 1;
        } else if optional_presence_pair(args, argument_index, parameter) {
            mappings.push(PublicCppParameterMapping {
                cpp_parameter_index: parameter_index,
                public_type: Some(args[argument_index + 1].ty.as_str()),
                presence_name: Some(argument.name.as_str()),
                public_argument_index: public_start + argument_index,
                scalar_choice: None,
            });
            argument_index += 2;
            parameter_index += 1;
        } else if argument.fields.is_empty() {
            mappings.push(PublicCppParameterMapping {
                cpp_parameter_index: parameter_index,
                public_type: Some(argument.ty.as_str()),
                presence_name: None,
                public_argument_index: public_start + argument_index,
                scalar_choice: None,
            });
            argument_index += 1;
            parameter_index += 1;
        } else {
            for _ in &argument.fields {
                if parameter_index >= symbol.parameters.len() {
                    break;
                }
                mappings.push(PublicCppParameterMapping {
                    cpp_parameter_index: parameter_index,
                    public_type: None,
                    presence_name: None,
                    public_argument_index: public_start + argument_index,
                    scalar_choice: None,
                });
                parameter_index += 1;
            }
            argument_index += 1;
        }
    }
    mappings
}

/// Builds the validated explicit permutation in stable public-call order.
pub(super) fn explicit_public_cpp_parameter_mappings<'a>(
    args: &'a [NativeBindingArg],
    public_start: usize,
    symbol: &'a CppSymbol,
) -> Vec<PublicCppParameterMapping<'a>> {
    let mut mappings = Vec::new();
    let mut argument_index = 0;
    while argument_index < args.len() {
        let argument = &args[argument_index];
        if argument.cpp_ignore {
            argument_index += 1;
            continue;
        }
        let Some(parameter_index) = argument.cpp_parameter.as_deref().and_then(|target| {
            symbol
                .parameters
                .iter()
                .position(|parameter| parameter.name == target)
        }) else {
            break;
        };
        let parameter = &symbol.parameters[parameter_index];
        if scalar_choice_triple(args, argument_index, parameter) {
            mappings.push(PublicCppParameterMapping {
                cpp_parameter_index: parameter_index,
                public_type: None,
                presence_name: None,
                public_argument_index: public_start + argument_index,
                scalar_choice: Some(PublicScalarChoice {
                    tag_name: argument.name.as_str(),
                    integer_name: args[argument_index + 1].name.as_str(),
                    floating_name: args[argument_index + 2].name.as_str(),
                }),
            });
            argument_index += 3;
        } else if optional_presence_pair(args, argument_index, parameter) {
            mappings.push(PublicCppParameterMapping {
                cpp_parameter_index: parameter_index,
                public_type: Some(args[argument_index + 1].ty.as_str()),
                presence_name: Some(argument.name.as_str()),
                public_argument_index: public_start + argument_index,
                scalar_choice: None,
            });
            argument_index += 2;
        } else {
            mappings.push(PublicCppParameterMapping {
                cpp_parameter_index: parameter_index,
                public_type: Some(argument.ty.as_str()),
                presence_name: None,
                public_argument_index: public_start + argument_index,
                scalar_choice: None,
            });
            argument_index += 1;
        }
    }
    mappings
}

/// Chooses an explicit `std::nullopt` for an optional that is absent from the
/// reviewed public contract and appears before the next public argument.
/// Exact parameter names disambiguate equal resource shapes (`weight`/`bias`,
/// `prepend`/`append`); bridge-compatible scalar shapes cover harmless
/// upstream spelling differences (`epsilon`/`eps`). A C++ default is not
/// required: generated ATen APIs frequently expose a required optional whose
/// explicit null value is the operation's absent branch.
pub(super) fn should_omit_default_optional_parameter(
    args: &[NativeBindingArg],
    argument_index: usize,
    parameters: &[CppParameter],
    parameter_index: usize,
) -> bool {
    let Some(argument) = args.get(argument_index) else {
        return false;
    };
    let Some(parameter) = parameters.get(parameter_index) else {
        return false;
    };
    if !is_explicitly_omittable_optional(&parameter.ty) {
        return false;
    }
    // A reviewed `(has_<parameter>, <parameter>)` group must be consumed by
    // the presence-pair matcher below.  Do not let the broad optional-omission
    // heuristic reinterpret a Bool presence flag as a later optional Bool
    // (for example `pin_memory`) and skip the enum-valued option entirely.
    if optional_presence_pair(args, argument_index, parameter) {
        return false;
    }
    let later = &parameters[parameter_index + 1..];
    if !public_argument_name_matches_cpp(&argument.name, &parameter.name)
        && later
            .iter()
            .any(|candidate| public_argument_name_matches_cpp(&argument.name, &candidate.name))
    {
        return true;
    }
    !coarse_public_argument_matches_cpp(argument, parameter)
        && later
            .iter()
            .any(|candidate| coarse_public_argument_matches_cpp(argument, candidate))
}

/// Matches stable public parameter spellings to narrowly equivalent upstream
/// abbreviations used by generated ATen declarations.
pub(super) fn public_argument_name_matches_cpp(public: &str, cpp: &str) -> bool {
    public == cpp
        || matches!((public, cpp), ("dimension", "dim") | ("dimensions", "dim"))
        || public
            .strip_prefix("positive_")
            .is_some_and(|suffix| cpp == format!("pos_{suffix}"))
}

/// Performs manifest-independent alignment only. Full reviewed type validation
/// still runs afterward and remains authoritative.
pub(super) fn coarse_public_argument_matches_cpp(
    argument: &NativeBindingArg,
    parameter: &CppParameter,
) -> bool {
    if !argument.fields.is_empty() {
        return false;
    }
    if terlan_primitive_matches_cpp(&argument.ty, &parameter.ty)
        || optional_cpp_inner_type(&parameter.ty)
            .is_some_and(|inner| terlan_primitive_matches_optional_cpp(&argument.ty, inner))
    {
        return true;
    }
    if matches!(argument.ty.as_str(), "Int" | "Float")
        && (is_cpp_scalar_type(&parameter.ty) || is_optional_cpp_scalar_type(&parameter.ty))
    {
        return true;
    }
    if argument.ty == "Int" && optional_cpp_inner_type(&parameter.ty).is_some() {
        return optional_cpp_inner_type(&parameter.ty)
            .is_none_or(|inner| cpp_short_component(inner) != "Tensor");
    }
    if argument.ty == "String" {
        return is_rust_str_type(&parameter.ty)
            || is_cpp_string_view_type(&parameter.ty)
            || is_optional_cpp_string_view_type(&parameter.ty);
    }
    if argument.ty == "List[Int]" {
        return is_i64_slice_type(&parameter.ty)
            || is_i64_array_ref_type(&parameter.ty)
            || is_optional_i64_array_ref_type(&parameter.ty);
    }
    let public_name = argument
        .ty
        .rsplit(['.', ':'])
        .find(|value| !value.is_empty())
        .unwrap_or(argument.ty.as_str());
    optional_cpp_inner_type(&parameter.ty)
        .or_else(|| borrowed_const_record_name(&parameter.ty))
        .or_else(|| mutable_record_reference_name(&parameter.ty))
        .is_some_and(|cpp_name| cpp_short_component(cpp_name) == public_name)
}

pub(super) fn cpp_short_component(value: &str) -> &str {
    value.rsplit("::").next().unwrap_or(value).trim()
}

/// Recognizes a conventional tagged public encoding of one ATen/c10 Scalar.
///
/// The public prefix is structural rather than tied to the extracted C++
/// parameter name. This keeps a stable package contract valid when an upstream
/// declaration uses a more specific spelling such as `fill_value`.
pub(super) fn scalar_choice_triple(
    args: &[NativeBindingArg],
    argument_index: usize,
    parameter: &CppParameter,
) -> bool {
    if !is_cpp_scalar_type(&parameter.ty) && !is_optional_cpp_scalar_type(&parameter.ty) {
        return false;
    }
    let Some(tag) = args.get(argument_index) else {
        return false;
    };
    let Some(integer) = args.get(argument_index + 1) else {
        return false;
    };
    let Some(floating) = args.get(argument_index + 2) else {
        return false;
    };
    let Some((prefix, integer_suffix, floating_suffix)) = tag
        .name
        .strip_suffix("_is_int")
        .map(|prefix| (prefix, "int", "float"))
        .or_else(|| {
            tag.name
                .strip_suffix("_is_integer")
                .map(|prefix| (prefix, "integer", "floating"))
        })
    else {
        return false;
    };
    let integer_name = integer.name.as_str();
    let floating_name = floating.name.as_str();
    let names_match = integer_name == format!("{prefix}_{integer_suffix}")
        && floating_name == format!("{prefix}_{floating_suffix}")
        || integer_name == format!("integer_{prefix}")
            && floating_name == format!("floating_{prefix}");
    !prefix.is_empty()
        && tag.fields.is_empty()
        && tag.ty == "Bool"
        && integer.fields.is_empty()
        && integer.ty == "Int"
        && floating.fields.is_empty()
        && floating.ty == "Float"
        && names_match
}

/// Recognizes the zero-ceremony `has_<parameter>, <parameter>` public encoding
/// for one extracted `std::optional<T>` input.
pub(super) fn optional_presence_pair(
    args: &[NativeBindingArg],
    argument_index: usize,
    parameter: &CppParameter,
) -> bool {
    optional_cpp_inner_type(&parameter.ty).is_some()
        && args.get(argument_index).is_some_and(|presence| {
            presence.fields.is_empty()
                && presence.ty == "Bool"
                && presence.name == format!("has_{}", parameter.name)
        })
        && args
            .get(argument_index + 1)
            .is_some_and(|value| value.fields.is_empty() && value.name == parameter.name)
}

/// Matches a public primitive to the inner type of `std::optional<T>`.
pub(super) fn terlan_primitive_matches_optional_cpp(ty: &str, inner: &str) -> bool {
    let inner = compact_cpp_spelling(inner);
    match ty {
        "Int" => matches!(
            inner.as_str(),
            "long" | "longlong" | "std::int64_t" | "int64_t"
        ),
        "Float" => inner == "double",
        "Bool" => inner == "bool",
        _ => false,
    }
}
