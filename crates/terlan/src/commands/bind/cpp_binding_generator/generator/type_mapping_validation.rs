use super::*;

/// Validates the getter set used to construct one copied record result.
pub(super) fn validate_value_projection(
    function: &NativeBindingFunction,
    module: &NativeBindingModule,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), String> {
    let record = module
        .types
        .iter()
        .find(|ty| {
            ty.kind == NativeBindingTypeKind::ValueRecord
                && terlan_type_matches(&function.returns, &ty.name)
        })
        .ok_or_else(|| {
            format!(
                "value projection `{}` must return a module-owned value record",
                function.name
            )
        })?;
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
                "value projection `{}` requires an opaque resource as its first argument",
                function.name
            )
        })?;
    let resource_symbol = symbols
        .declarations
        .get(resource.cpp_symbol.as_str())
        .expect("validated projection resource symbol");
    if function.args.len() != 1 {
        return Err(format!(
            "value projection `{}` currently requires exactly one resource argument",
            function.name
        ));
    }
    validate_projection_fields(function, record, resource_symbol, symbols)
}

/// Validates a temporary owned C++ value projected directly into a Terlan record.
pub(super) fn validate_owned_value_projection(
    function: &NativeBindingFunction,
    producer: &CppSymbol,
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), String> {
    let record = modules
        .iter()
        .flat_map(|module| &module.types)
        .find(|ty| {
            ty.kind == NativeBindingTypeKind::ValueRecord
                && terlan_type_matches(&function.returns, &ty.name)
        })
        .ok_or_else(|| {
            format!(
                "owned value projection `{}` must return a package-owned value record",
                function.name
            )
        })?;
    let record_symbol = symbols
        .declarations
        .get(record.cpp_symbol.as_str())
        .expect("validated owned projection record symbol");
    let returns_record = producer
        .returns
        .as_ref()
        .and_then(owned_unique_ptr_name)
        .is_some_and(|name| cpp_name_matches(name, &record_symbol.cpp_name));
    if producer.kind != CppSymbolKind::Function || !returns_record {
        return Err(format!(
            "owned value projection `{}` requires a free function returning std::unique_ptr<{}>",
            function.name, record_symbol.cpp_name
        ));
    }
    if function.resource != NativeResourcePolicy::Value {
        return Err(format!(
            "owned value projection `{}` must expose a copied value result",
            function.name
        ));
    }
    validate_function_argument_mapping(function, producer, modules, symbols)?;
    validate_projection_fields(function, record, record_symbol, symbols)
}

/// Validates complete primitive getter projections for one copied record.
pub(super) fn validate_projection_fields(
    function: &NativeBindingFunction,
    record: &NativeBindingType,
    receiver: &CppSymbol,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), String> {
    if function.projections.len() != record.fields.len() {
        return Err(format!(
            "value projection `{}` requires exactly {} field projections",
            function.name,
            record.fields.len()
        ));
    }
    let mut projected = BTreeSet::new();
    for projection in &function.projections {
        let field = record
            .fields
            .iter()
            .find(|field| field.name == projection.field)
            .ok_or_else(|| {
                format!(
                    "value projection `{}` references unknown field `{}`",
                    function.name, projection.field
                )
            })?;
        if !projected.insert(field.name.as_str()) {
            return Err(format!(
                "value projection `{}` duplicates field `{}`",
                function.name, field.name
            ));
        }
        let symbol = symbols
            .declarations
            .get(projection.cpp_symbol.as_str())
            .ok_or_else(|| {
                format!(
                    "value projection `{}` references unknown C++ symbol `{}`",
                    function.name, projection.cpp_symbol
                )
            })?;
        if !symbols.is_bindable(&projection.cpp_symbol)
            || symbol.kind != CppSymbolKind::Method
            || !symbol
                .receiver
                .as_deref()
                .is_some_and(|name| cpp_name_matches(name, &receiver.cpp_name))
            || !symbol.parameters.is_empty()
            || !symbol
                .returns
                .as_ref()
                .is_some_and(|returns| projection_type_matches(&field.ty, returns))
        {
            return Err(format!(
                "value projection `{}.{}` requires a bindable zero-argument {} getter",
                function.name, field.name, field.ty
            ));
        }
    }
    Ok(())
}

/// Returns the uniquely owned C++ pointee name represented by one metadata type.
pub(super) fn owned_unique_ptr_name(ty: &CppTypeMetadata) -> Option<&str> {
    ty.canonical
        .strip_prefix("std::unique_ptr<")
        .and_then(|value| value.strip_suffix('>'))
}

/// Matches one Terlan primitive record field to its extracted C++ getter type.
pub(super) fn projection_type_matches(field: &str, returns: &CppTypeMetadata) -> bool {
    match field {
        "Int" => is_i64_type(returns),
        "Float" => returns.canonical == "double",
        "Bool" => returns.canonical == "bool",
        _ => false,
    }
}

pub(super) fn validate_mapping_policy<'a>(
    mapping: &'a CppMappingPolicy,
    declarations: &BTreeMap<&str, &CppSymbol>,
) -> Result<BTreeMap<&'a str, &'a CppSymbolPolicy>, String> {
    let mut policies = BTreeMap::new();
    for policy in &mapping.symbols {
        let symbol = declarations.get(policy.symbol.as_str()).ok_or_else(|| {
            format!(
                "C++ mapping policy references unknown extracted symbol `{}`",
                policy.symbol
            )
        })?;
        if policies.insert(policy.symbol.as_str(), policy).is_some() {
            return Err(format!(
                "duplicate C++ mapping policy for symbol `{}`",
                policy.symbol
            ));
        }
        validate_symbol_policy(symbol, policy)?;
    }
    for symbol in declarations.keys() {
        if !policies.contains_key(symbol) {
            return Err(format!(
                "error[cpp.mapping.missing]: extracted C++ symbol `{symbol}` has no package-owned mapping policy"
            ));
        }
    }
    Ok(policies)
}

pub(super) fn validate_symbol_policy(
    symbol: &CppSymbol,
    policy: &CppSymbolPolicy,
) -> Result<(), String> {
    match policy.disposition {
        CppSymbolDisposition::Reject => {
            let rejection = policy.rejection.as_ref().ok_or_else(|| {
                format!(
                    "rejected C++ symbol `{}` requires a stable rejection policy",
                    symbol.id
                )
            })?;
            if rejection.detail.trim().is_empty() {
                return Err(format!(
                    "rejected C++ symbol `{}` requires stable rejection detail",
                    symbol.id
                ));
            }
            if policy.ownership.is_some()
                || policy.thread_safety.is_some()
                || policy.exception.is_some()
            {
                return Err(format!(
                    "rejected C++ symbol `{}` cannot carry binding policy",
                    symbol.id
                ));
            }
        }
        CppSymbolDisposition::Bind => {
            if policy.rejection.is_some() {
                return Err(format!(
                    "bindable C++ symbol `{}` cannot carry a rejection policy",
                    symbol.id
                ));
            }
            if symbol.kind == CppSymbolKind::Record
                && (policy.ownership.is_none() || policy.thread_safety.is_none())
            {
                return Err(stable_shape_error(
                    symbol,
                    UnsupportedCppShape::UnknownOwnership,
                ));
            }
            if matches!(symbol.kind, CppSymbolKind::Record | CppSymbolKind::Enum)
                && policy.exception.is_some()
            {
                return Err(format!(
                    "C++ type symbol `{}` cannot declare callable exception policy",
                    symbol.id
                ));
            }
            if let Some(exception) = &policy.exception {
                validate_lower_identifier("exception error code", &exception.error_code)?;
                if exception.message.trim().is_empty()
                    || exception
                        .message
                        .chars()
                        .any(|ch| matches!(ch, '\0' | '\n' | '\r'))
                {
                    return Err(format!(
                        "C++ exception policy for `{}` requires a stable one-line message",
                        symbol.id
                    ));
                }
            }
            if symbol.noexcept && policy.exception.is_some() {
                return Err(format!(
                    "noexcept C++ symbol `{}` cannot declare exception containment policy",
                    symbol.id
                ));
            }
            validate_bindable_cpp_symbol(symbol, policy.exception.is_some())?;
        }
    }
    Ok(())
}

pub(super) fn validate_bindable_cpp_symbol(
    symbol: &CppSymbol,
    exception_contained: bool,
) -> Result<(), String> {
    if symbol.kind == CppSymbolKind::Macro {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCppShape::UnsupportedMacro,
        ));
    }
    if !symbol.template_parameters.is_empty()
        || symbol
            .returns
            .as_ref()
            .is_some_and(|ty| ty.template_dependent)
        || symbol
            .parameters
            .iter()
            .any(|parameter| parameter.ty.template_dependent)
    {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCppShape::UnsupportedTemplate,
        ));
    }
    if symbol.variadic {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCppShape::UnsupportedVariadicFunction,
        ));
    }
    if symbol.kind != CppSymbolKind::Record && !symbol.inheritance.is_empty() {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCppShape::UnsupportedInheritance,
        ));
    }
    if !matches!(symbol.kind, CppSymbolKind::Record | CppSymbolKind::Enum)
        && !symbol.noexcept
        && !exception_contained
    {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCppShape::ExceptionBoundary,
        ));
    }
    if let Some(returns) = &symbol.returns {
        if !is_by_value_record_candidate(returns)
            && mutable_record_reference_name(returns).is_none()
        {
            validate_cpp_type_shape(symbol, returns)?;
        }
    }
    for parameter in &symbol.parameters {
        if parameter.default.is_none()
            && !is_generated_adapter_input(&parameter.ty)
            && mutable_record_reference_name(&parameter.ty).is_none()
        {
            validate_cpp_type_shape(symbol, &parameter.ty)?;
        }
    }
    Ok(())
}

/// Recognizes an otherwise-unmapped named C++ value which may be adapted only
/// after the public contract proves that it is a package-owned opaque record.
pub(super) fn is_by_value_record_candidate(cpp_type: &CppTypeMetadata) -> bool {
    cpp_type.pointer_depth == 0
        && cpp_type.reference == CppReferenceKind::None
        && !cpp_type.function_pointer
        && !cpp_type.template_dependent
        && !cpp_type.enum_type
        && !is_supported_cxx_type(cpp_type)
}

pub(super) fn validate_cpp_type_metadata(
    symbol: &CppSymbol,
    position: &str,
    cpp_type: &CppTypeMetadata,
) -> Result<(), String> {
    if cpp_type.spelling.trim().is_empty() || cpp_type.canonical.trim().is_empty() {
        return Err(format!(
            "structured C++ type metadata for `{}::{position}` requires declared and canonical spellings",
            symbol.id
        ));
    }
    Ok(())
}

pub(super) fn validate_cpp_type_shape(
    symbol: &CppSymbol,
    cpp_type: &CppTypeMetadata,
) -> Result<(), String> {
    if cpp_type.function_pointer {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCppShape::UnsupportedCallbackShape,
        ));
    }
    if cpp_type.pointer_depth > 0 {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCppShape::RawPointerOwnership,
        ));
    }
    if cpp_type.reference != CppReferenceKind::None
        && borrowed_const_record_name(cpp_type).is_none()
        && cpp_array_ref_element(cpp_type).is_none()
    {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCppShape::ReferenceLifetimeAmbiguity,
        ));
    }
    if !cpp_type.enum_type && !is_supported_cxx_type(cpp_type) {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCppShape::UnmappedType,
        ));
    }
    Ok(())
}

/// Requires every public argument to match its extracted C++ parameter shape.
pub(super) fn validate_function_argument_mapping(
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), String> {
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
        return Err(format!(
            "error[cpp.lifetime.mutable_alias]: mutable method `{}` cannot borrow a second opaque resource",
            function.name
        ));
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
            return Err(format!(
                "error[cpp.type.resource_list_prepend]: function `{}` argument `{}` must immediately follow the opaque resource copied into its resource list",
                function.name, argument.name
            ));
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
                return Err(format!(
                    "error[cpp.type.resource_list_prepend]: function `{}` arguments `{}` and `{}` must be an immutable opaque resource followed by a matching immutable resource list without defaults or field projections",
                    function.name, argument.name, list.name
                ));
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
            return Err(format!(
                "error[cpp.type.record_argument_fields]: function `{}` argument `{}` must map all {} fields from `{}`",
                function.name,
                argument.name,
                record.fields.len(),
                record.name
            ));
        }
        let mut public_fields = BTreeSet::new();
        let mut cpp_parameters = BTreeSet::new();
        for mapping in &argument.fields {
            if !public_fields.insert(mapping.field.as_str()) {
                return Err(format!(
                    "error[cpp.type.record_argument_duplicate]: function `{}` argument `{}` maps field `{}` more than once",
                    function.name, argument.name, mapping.field
                ));
            }
            if !cpp_parameters.insert(mapping.cpp_parameter.as_str()) {
                return Err(format!(
                    "error[cpp.type.record_parameter_duplicate]: function `{}` argument `{}` maps C++ parameter `{}` more than once",
                    function.name, argument.name, mapping.cpp_parameter
                ));
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
                return Err(format!(
                    "error[cpp.type.record_parameter_order]: function `{}` field `{}.{}` must map the next extracted C++ parameter `{}`, not `{}`",
                    function.name,
                    argument.name,
                    mapping.field,
                    parameter.name,
                    mapping.cpp_parameter
                ));
            }
            if !terlan_primitive_matches_cpp(&field.ty, &parameter.ty) {
                return Err(format!(
                    "error[cpp.type.record_argument_mapping_mismatch]: function `{}` field `{}.{}` maps incompatible Terlan type `{}` to C++ parameter `{}`",
                    function.name,
                    argument.name,
                    mapping.field,
                    field.ty,
                    parameter.ty.canonical
                ));
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
        return Err(format!(
            "error[cpp.type.argument_count]: function `{}` maps {parameter_index} public scalar values but one of the remaining {} C++ parameters has no default",
            function.name,
            symbol.parameters.len() - parameter_index
        ));
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
fn validate_explicit_function_argument_mapping(
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
    args: &[NativeBindingArg],
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), String> {
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
                return Err(format!(
                    "error[cpp.type.ignored_parameter_shape]: function `{}` argument `{}` marked `cpp_ignore` must be an immutable copied value without `cpp_parameter`, resource-list prepending, or record-field expansion",
                    function.name, argument.name
                ));
            }
            argument_index += 1;
            continue;
        }
        if argument.prepend_resource || !argument.fields.is_empty() {
            return Err(format!(
                "error[cpp.type.explicit_parameter_shape]: function `{}` argument `{}` cannot combine `cpp_parameter` permutation with resource-list prepending or record-field expansion",
                function.name, argument.name
            ));
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
            return Err(format!(
                "error[cpp.type.explicit_parameter_duplicate]: function `{}` maps C++ parameter `{target}` more than once",
                function.name
            ));
        }
        let parameter = &symbol.parameters[parameter_index];
        if scalar_choice_triple(args, argument_index, parameter) {
            if args[argument_index + 1].cpp_parameter.is_some()
                || args[argument_index + 1].cpp_ignore
                || args[argument_index + 2].cpp_parameter.is_some()
                || args[argument_index + 2].cpp_ignore
            {
                return Err(format!(
                    "error[cpp.type.explicit_parameter_group]: function `{}` must name `cpp_parameter` only on the first field of tagged Scalar `{}`",
                    function.name, argument.name
                ));
            }
            argument_index += 3;
            continue;
        }
        if optional_presence_pair(args, argument_index, parameter) {
            if args[argument_index + 1].cpp_parameter.is_some()
                || args[argument_index + 1].cpp_ignore
            {
                return Err(format!(
                    "error[cpp.type.explicit_parameter_group]: function `{}` must name `cpp_parameter` only on optional presence flag `{}`",
                    function.name, argument.name
                ));
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
        return Err(format!(
            "error[cpp.type.explicit_parameter_incomplete]: function `{}` does not map extracted C++ parameters {}",
            function.name,
            missing.join(", ")
        ));
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
) -> Result<(), String> {
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
    Err(format!(
        "error[cpp.type.argument_mapping_mismatch]: function `{}` maps C++ parameter `{}` to incompatible Terlan type `{}`",
        function.name, parameter.ty.canonical, argument.ty
    ))
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

/// Requires each public result type to match the extracted C++ ownership and
/// container shape before any bridge source is emitted.
pub(super) fn validate_function_return_mapping(
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), String> {
    let returns = symbol.returns.as_ref();
    let canonical = returns
        .map(|returns| returns.canonical.as_str())
        .unwrap_or("void");
    let compatible =
        resource_tuple_return_resources(modules, &function.returns, symbol, &symbols.declarations)
            .is_some()
            || match function.returns.as_str() {
                "Unit" => {
                    canonical == "void"
                        || function.role == NativeFunctionRole::MutableFreeFunction
                            && returns
                                .and_then(|returns| {
                                    mutable_record_reference_name(returns)
                                        .or_else(|| borrowed_const_record_name(returns))
                                })
                                .is_some()
                        || function.role == NativeFunctionRole::MutableFreeFunction
                            && mutable_resource_tuple_return_matches(
                                function, symbol, modules, symbols,
                            )
                        || function.role == NativeFunctionRole::MutableMethod
                            && symbols
                                .policies
                                .get(symbol.id.as_str())
                                .is_some_and(|policy| policy.exception.is_some())
                            && returns
                                .and_then(|returns| {
                                    mutable_record_reference_name(returns)
                                        .or_else(|| borrowed_const_record_name(returns))
                                })
                                .zip(symbol.receiver.as_deref())
                                .is_some_and(|(returned, receiver)| {
                                    cpp_name_matches(returned, receiver)
                                })
                }
                "Int" => returns.is_some_and(is_i64_type),
                "Float" => canonical == "double",
                "Bool" => canonical == "bool",
                "String" => returns.is_some_and(is_owned_string_type),
                "std.vm.Bytes.Bytes" | "Bytes" => returns.is_some_and(is_owned_u8_vector_type),
                "List[Int]" => returns.is_some_and(is_owned_i64_vector_type),
                "List[Float]" => returns.is_some_and(is_owned_f64_vector_type),
                // Bool lists use an owned byte vector at the CXX boundary;
                // this avoids the specialised `std::vector<bool>` layout
                // while preserving the public boolean-list contract.
                "List[Bool]" => returns.is_some_and(is_owned_u8_vector_type),
                "List[String]" => returns.is_some_and(is_owned_string_vector_type),
                option
                    if option
                        .strip_prefix("Option[")
                        .and_then(|inner| inner.strip_suffix(']'))
                        .and_then(|inner| find_resource_type_in_modules(modules, inner.trim()))
                        .is_some_and(|resource| {
                            symbols
                                .declarations
                                .get(resource.cpp_symbol.as_str())
                                .is_some_and(|resource_symbol| {
                                    owned_unique_ptr_name(
                                        symbol.returns.as_ref().expect("known return"),
                                    )
                                    .is_some_and(|name| {
                                        cpp_name_matches(name, &resource_symbol.cpp_name)
                                    })
                                })
                        }) =>
                {
                    true
                }
                returns => modules.iter().flat_map(|module| &module.types).any(|ty| {
                    if ty.kind != NativeBindingTypeKind::OpaqueResource
                        || !terlan_type_matches(returns, &ty.name)
                    {
                        return false;
                    }
                    let Some(resource_symbol) = symbols.declarations.get(ty.cpp_symbol.as_str())
                    else {
                        return false;
                    };
                    owned_unique_ptr_name(symbol.returns.as_ref().expect("known return"))
                        .is_some_and(|name| cpp_name_matches(name, &resource_symbol.cpp_name))
                        || matches!(
                            function.role,
                            NativeFunctionRole::Constructor
                                | NativeFunctionRole::FreeFunction
                                | NativeFunctionRole::ImmutableMethod
                        ) && cpp_name_matches(canonical, &resource_symbol.cpp_name)
                }),
            };
    if !compatible {
        return Err(format!(
            "error[cpp.type.mapping_mismatch]: function `{}` maps C++ result `{canonical}` to incompatible Terlan type `{}`",
            function.name, function.returns
        ));
    }
    if matches!(
        function.resource,
        NativeResourcePolicy::OwnedHandle | NativeResourcePolicy::NullableHandle
    ) && !modules.iter().flat_map(|module| &module.types).any(|ty| {
        if ty.kind != NativeBindingTypeKind::OpaqueResource
            || !terlan_type_matches(&function.returns, &ty.name)
        {
            return false;
        }
        let Some(resource_symbol) = symbols.declarations.get(ty.cpp_symbol.as_str()) else {
            return false;
        };
        returns.is_some_and(|returns| {
            owned_unique_ptr_name(returns)
                .is_some_and(|name| cpp_name_matches(name, &resource_symbol.cpp_name))
                || matches!(
                    function.role,
                    NativeFunctionRole::Constructor
                        | NativeFunctionRole::FreeFunction
                        | NativeFunctionRole::ImmutableMethod
                ) && cpp_name_matches(&returns.canonical, &resource_symbol.cpp_name)
        })
    }) {
        return Err(format!(
            "error[cpp.ownership.handle_result]: function `{}` classifies `{canonical}` as `{}` but must return a package-owned resource by value or std::unique_ptr",
            function.name,
            resource_policy_name(&function.resource)
        ));
    }
    Ok(())
}

/// Proves a discarded C++ tuple result aliases every retained mutable resource
/// in public argument order. Generated mutation adapters never expose the alias
/// tuple, but this evidence prevents unrelated by-value tuples from being
/// mislabeled as `Unit`.
pub(super) fn mutable_resource_tuple_return_matches(
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> bool {
    let Some(elements) = symbol
        .returns
        .as_ref()
        .and_then(|returns| cpp_std_tuple_elements(&returns.canonical))
    else {
        return false;
    };
    let mutable_arguments = function
        .args
        .iter()
        .enumerate()
        .filter(|(index, argument)| *index == 0 || argument.mutable)
        .collect::<Vec<_>>();
    if elements.len() != mutable_arguments.len() {
        return false;
    }
    elements
        .into_iter()
        .zip(mutable_arguments)
        .all(|(element, (_, argument))| {
            let canonical = element
                .trim()
                .strip_prefix("const ")
                .unwrap_or(element.trim())
                .trim_end_matches('&')
                .trim();
            find_resource_type_in_modules(modules, &argument.ty)
                .and_then(|resource| symbols.declarations.get(resource.cpp_symbol.as_str()))
                .is_some_and(|resource| {
                    cpp_name_matches(canonical, &resource.cpp_name)
                        || cpp_name_matches(canonical, &resource.overload_set)
                })
        })
}

pub(super) fn is_supported_cxx_type(cpp_type: &CppTypeMetadata) -> bool {
    let canonical = cpp_type.canonical.as_str();
    canonical == "void"
        || is_i64_type(cpp_type)
        || canonical == "double"
        || canonical == "bool"
        || is_rust_str_type(cpp_type)
        || is_cpp_string_view_type(cpp_type)
        || is_u8_slice_type(cpp_type)
        || is_i64_slice_type(cpp_type)
        || is_f64_slice_type(cpp_type)
        || borrowed_const_record_name(cpp_type).is_some()
        || cpp_array_ref_element(cpp_type).is_some()
        || canonical
            .strip_prefix("std::unique_ptr<")
            .and_then(|value| value.strip_suffix('>'))
            .is_some_and(|inner| !inner.trim().is_empty())
}

/// Returns the element name from a supported immutable ATen/c10 list view.
pub(super) fn cpp_array_ref_element(cpp_type: &CppTypeMetadata) -> Option<&str> {
    if cpp_type.pointer_depth != 0 {
        return None;
    }
    let canonical = cpp_type.canonical.trim();
    let list = if cpp_type.reference == CppReferenceKind::None {
        canonical
    } else if cpp_type.reference == CppReferenceKind::Lvalue && cpp_type.is_const {
        canonical.strip_prefix("const ")?.strip_suffix('&')?.trim()
    } else {
        return None;
    };
    list.strip_prefix("c10::ArrayRef<")
        .or_else(|| list.strip_prefix("at::ArrayRef<"))
        .or_else(|| list.strip_prefix("c10::IListRef<"))
        .or_else(|| list.strip_prefix("at::IListRef<"))
        .and_then(|value| value.strip_suffix('>'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Recognizes an immutable integer view constructible from `(data, size)`.
/// Clang canonicalizes ATen's `IntArrayRef` alias to `c10::ArrayRef<long>`.
pub(super) fn is_i64_array_ref_type(cpp_type: &CppTypeMetadata) -> bool {
    if cpp_type.pointer_depth != 0 || cpp_type.reference != CppReferenceKind::None {
        return false;
    }
    let spelling = compact_cpp_spelling(&cpp_type.spelling);
    let canonical = compact_cpp_spelling(&cpp_type.canonical);
    if canonical.contains("OptionalArrayRef<") {
        return false;
    }
    matches!(
        spelling.as_str(),
        "at::IntArrayRef" | "c10::IntArrayRef" | "c10::ArrayRef<std::int64_t>"
    ) || ["long", "longlong", "std::int64_t", "int64_t"]
        .iter()
        .any(|element| canonical.ends_with(&format!("ArrayRef<{element}>")))
}

/// Recognizes c10's optional immutable integer view. CXX cannot bridge this
/// template directly, so generated adapters receive a Rust slice and construct
/// the exact optional ArrayRef on the C++ side.
pub(super) fn is_optional_i64_array_ref_type(cpp_type: &CppTypeMetadata) -> bool {
    if cpp_type.pointer_depth != 0 || cpp_type.reference != CppReferenceKind::None {
        return false;
    }
    let spelling = compact_cpp_spelling(&cpp_type.spelling);
    let canonical = compact_cpp_spelling(&cpp_type.canonical);
    matches!(
        spelling.as_str(),
        "at::OptionalIntArrayRef" | "c10::OptionalIntArrayRef" | "OptionalIntArrayRef"
    ) || ["long", "longlong", "std::int64_t", "int64_t"]
        .iter()
        .any(|element| canonical.ends_with(&format!("OptionalArrayRef<{element}>")))
}

/// Recognizes ATen/c10's dynamically typed numeric scalar value.
pub(super) fn is_cpp_scalar_type(cpp_type: &CppTypeMetadata) -> bool {
    let canonical = compact_cpp_spelling(&cpp_type.canonical);
    matches!(
        canonical.as_str(),
        "c10::Scalar" | "constc10::Scalar&" | "at::Scalar" | "constat::Scalar&"
    )
}

/// Recognizes an optional ATen/c10 dynamically typed numeric scalar.
///
/// Historical package surfaces commonly expose separate `Int` and `Float`
/// overloads while generated ATen declarations receive
/// `std::optional<Scalar>`. The generated adapter constructs the present
/// optional value; an absent public argument continues to lower to
/// `std::nullopt` through the ordinary optional-default path.
pub(super) fn is_optional_cpp_scalar_type(cpp_type: &CppTypeMetadata) -> bool {
    optional_cpp_inner_type(cpp_type).is_some_and(|inner| {
        matches!(
            compact_cpp_spelling(inner).as_str(),
            "c10::Scalar" | "at::Scalar"
        )
    })
}

/// Returns the type wrapped by a by-value or immutable-reference
/// `std::optional<T>` declaration. CXX adapters copy the optional's contained
/// value before entering the selected upstream callable.
pub(super) fn optional_cpp_inner_type(cpp_type: &CppTypeMetadata) -> Option<&str> {
    if cpp_type.pointer_depth != 0 {
        return None;
    }
    let canonical = match cpp_type.reference {
        CppReferenceKind::None => cpp_type.canonical.trim(),
        CppReferenceKind::Lvalue if cpp_type.is_const => cpp_type
            .canonical
            .trim()
            .strip_prefix("const ")?
            .strip_suffix('&')?
            .trim(),
        _ => return None,
    };
    canonical
        .strip_prefix("std::optional<")
        .and_then(|value| value.strip_suffix('>'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// One extracted parameter's aligned public representation.
pub(super) struct PublicCppParameterMapping<'a> {
    /// Index of the corresponding parameter in the extracted C++ declaration.
    pub(super) cpp_parameter_index: usize,
    /// Public scalar type used to select a bridge-safe lowering.
    pub(super) public_type: Option<&'a str>,
    /// Optional public presence flag for `(has_value, value)` conventions.
    pub(super) presence_name: Option<&'a str>,
    /// Index of the first corresponding argument in the complete public call.
    pub(super) public_argument_index: usize,
    /// Optional tagged integer/floating representation of one C++ Scalar.
    pub(super) scalar_choice: Option<PublicScalarChoice<'a>>,
}

/// Public fields used to construct one exact C++ numeric Scalar.
pub(super) struct PublicScalarChoice<'a> {
    pub(super) tag_name: &'a str,
    pub(super) integer_name: &'a str,
    pub(super) floating_name: &'a str,
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
fn explicit_public_cpp_parameter_mappings<'a>(
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
fn should_omit_default_optional_parameter(
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
fn public_argument_name_matches_cpp(public: &str, cpp: &str) -> bool {
    public == cpp
        || matches!((public, cpp), ("dimension", "dim") | ("dimensions", "dim"))
        || public
            .strip_prefix("positive_")
            .is_some_and(|suffix| cpp == format!("pos_{suffix}"))
}

/// Performs manifest-independent alignment only. Full reviewed type validation
/// still runs afterward and remains authoritative.
fn coarse_public_argument_matches_cpp(
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
        return !optional_cpp_inner_type(&parameter.ty)
            .is_some_and(|inner| cpp_short_component(inner) == "Tensor");
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

fn cpp_short_component(value: &str) -> &str {
    value.rsplit("::").next().unwrap_or(value).trim()
}

/// Recognizes a conventional tagged public encoding of one ATen/c10 Scalar.
///
/// The public prefix is structural rather than tied to the extracted C++
/// parameter name. This keeps a stable package contract valid when an upstream
/// declaration uses a more specific spelling such as `fill_value`.
fn scalar_choice_triple(
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
fn optional_presence_pair(
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
fn terlan_primitive_matches_optional_cpp(ty: &str, inner: &str) -> bool {
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

/// Returns whether generated C++ can lower a CXX-compatible borrowed input.
pub(super) fn is_generated_adapter_input(cpp_type: &CppTypeMetadata) -> bool {
    is_i64_array_ref_type(cpp_type)
        || is_optional_i64_array_ref_type(cpp_type)
        || cpp_array_ref_element(cpp_type).is_some()
        || optional_cpp_inner_type(cpp_type).is_some()
}

/// Returns the number of extracted parameters supplied by public arguments.
pub(super) fn mapped_cpp_parameter_count(
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
) -> usize {
    let mut count = public_cpp_parameter_mappings(function, symbol)
        .iter()
        .map(|mapping| mapping.cpp_parameter_index + 1)
        .max()
        .unwrap_or(0);
    // Exact function-pointer calls cannot rely on declaration-level defaults:
    // selecting an overload fixes the full parameter list, so every trailing
    // optional must be materialized as an explicit absent value.
    while symbol
        .parameters
        .get(count)
        .is_some_and(|parameter| is_explicitly_omittable_optional(&parameter.ty))
    {
        count += 1;
    }
    count
}

/// Returns whether generated C++ can supply a reviewed absent value without a
/// package-authored wrapper or a declaration-level default.
pub(super) fn is_explicitly_omittable_optional(cpp_type: &CppTypeMetadata) -> bool {
    optional_cpp_inner_type(cpp_type).is_some() || is_optional_i64_array_ref_type(cpp_type)
}

/// Returns a whitespace-insensitive declared C++ type spelling.
pub(super) fn compact_cpp_spelling(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

/// Returns the named record borrowed through one immutable C++ reference.
pub(super) fn borrowed_const_record_name(cpp_type: &CppTypeMetadata) -> Option<&str> {
    if !cpp_type.is_const
        || cpp_type.pointer_depth != 0
        || cpp_type.reference != CppReferenceKind::Lvalue
    {
        return None;
    }
    cpp_type
        .canonical
        .trim()
        .strip_prefix("const ")
        .and_then(|value| value.trim().strip_suffix('&'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Returns the named record borrowed through one mutable C++ lvalue reference.
pub(super) fn mutable_record_reference_name(cpp_type: &CppTypeMetadata) -> Option<&str> {
    if cpp_type.is_const
        || cpp_type.pointer_depth != 0
        || cpp_type.reference != CppReferenceKind::Lvalue
        || cpp_type.function_pointer
        || cpp_type.template_dependent
    {
        return None;
    }
    cpp_type
        .canonical
        .trim()
        .strip_suffix('&')
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Matches an extractor-qualified C++ name to its declaration-local name.
pub(super) fn cpp_name_matches(value: &str, expected: &str) -> bool {
    value == expected || value.ends_with(&format!("::{expected}"))
}

/// Returns whether an extracted resource record is the requested receiver or
/// publicly exposes that receiver through an extracted base-class relation.
pub(super) fn cpp_record_supports_receiver(record: &CppSymbol, receiver: &str) -> bool {
    cpp_name_matches(receiver, &record.cpp_name)
        || cpp_name_matches(receiver, &record.overload_set)
        || record.public_inheritance.iter().any(|base| {
            let base = base
                .trim()
                .strip_prefix("class ")
                .or_else(|| base.trim().strip_prefix("struct "))
                .unwrap_or_else(|| base.trim());
            cpp_name_matches(receiver, base) || cpp_name_matches(base, receiver)
        })
}

/// Returns the declaration-local segment of one extractor-qualified C++ name.
pub(super) fn cpp_short_name(value: &str) -> &str {
    value.rsplit("::").next().unwrap_or(value)
}

/// Recognizes a declared fixed-width signed 64-bit alias across Clang targets.
pub(super) fn is_i64_type(cpp_type: &CppTypeMetadata) -> bool {
    matches!(
        compact_cpp_spelling(&cpp_type.spelling).as_str(),
        "std::int64_t" | "int64_t"
    ) || cpp_type.canonical == "std::int64_t"
}

/// Recognizes the CXX borrowed UTF-8 string argument type.
pub(super) fn is_rust_str_type(cpp_type: &CppTypeMetadata) -> bool {
    compact_cpp_spelling(&cpp_type.spelling) == "rust::Str" || cpp_type.canonical == "rust::Str"
}

/// Recognizes an immutable standard-library string view accepted through a
/// generated call-scoped `rust::Str` parameter.
pub(super) fn is_cpp_string_view_type(cpp_type: &CppTypeMetadata) -> bool {
    if cpp_type.pointer_depth != 0 || cpp_type.reference != CppReferenceKind::None {
        return false;
    }
    matches!(
        compact_cpp_spelling(&cpp_type.canonical).as_str(),
        "std::basic_string_view<char>" | "std::string_view"
    ) || matches!(
        compact_cpp_spelling(&cpp_type.spelling).as_str(),
        "c10::string_view" | "std::string_view" | "std::basic_string_view<char>"
    )
}

/// Recognizes an optional immutable standard-library string view supplied as
/// one present, call-scoped `rust::Str` value.
pub(super) fn is_optional_cpp_string_view_type(cpp_type: &CppTypeMetadata) -> bool {
    optional_cpp_inner_type(cpp_type).is_some_and(|inner| {
        matches!(
            compact_cpp_spelling(inner).as_str(),
            "std::basic_string_view<char>" | "std::string_view" | "c10::string_view"
        )
    })
}

/// Recognizes a CXX borrowed unsigned-byte slice after typedef canonicalization.
pub(super) fn is_u8_slice_type(cpp_type: &CppTypeMetadata) -> bool {
    matches!(
        compact_cpp_spelling(&cpp_type.spelling).as_str(),
        "rust::Slice<conststd::uint8_t>" | "rust::Slice<constuint8_t>"
    ) || cpp_type.canonical == "rust::Slice<const std::uint8_t>"
}

/// Recognizes a CXX borrowed signed 64-bit slice after typedef canonicalization.
pub(super) fn is_i64_slice_type(cpp_type: &CppTypeMetadata) -> bool {
    matches!(
        compact_cpp_spelling(&cpp_type.spelling).as_str(),
        "rust::Slice<conststd::int64_t>" | "rust::Slice<constint64_t>"
    ) || cpp_type.canonical == "rust::Slice<const std::int64_t>"
}

/// Recognizes a CXX borrowed double slice.
pub(super) fn is_f64_slice_type(cpp_type: &CppTypeMetadata) -> bool {
    compact_cpp_spelling(&cpp_type.spelling) == "rust::Slice<constdouble>"
        || cpp_type.canonical == "rust::Slice<const double>"
}

/// Recognizes an owned standard string result from its declared contract.
pub(super) fn is_owned_string_type(cpp_type: &CppTypeMetadata) -> bool {
    compact_cpp_spelling(&cpp_type.spelling) == "std::unique_ptr<std::string>"
}

/// Recognizes an owned unsigned-byte vector despite canonical typedef expansion.
pub(super) fn is_owned_u8_vector_type(cpp_type: &CppTypeMetadata) -> bool {
    compact_cpp_spelling(&cpp_type.spelling) == "std::unique_ptr<std::vector<std::uint8_t>>"
}

/// Recognizes an owned signed 64-bit vector despite target-specific aliases.
pub(super) fn is_owned_i64_vector_type(cpp_type: &CppTypeMetadata) -> bool {
    compact_cpp_spelling(&cpp_type.spelling) == "std::unique_ptr<std::vector<std::int64_t>>"
}

/// Recognizes an owned double vector used for copied floating-point lists.
pub(super) fn is_owned_f64_vector_type(cpp_type: &CppTypeMetadata) -> bool {
    compact_cpp_spelling(&cpp_type.spelling) == "std::unique_ptr<std::vector<double>>"
}

/// Recognizes an owned string vector used for copied string lists.
pub(super) fn is_owned_string_vector_type(cpp_type: &CppTypeMetadata) -> bool {
    compact_cpp_spelling(&cpp_type.spelling) == "std::unique_ptr<std::vector<std::string>>"
}

pub(super) fn stable_shape_error(symbol: &CppSymbol, shape: UnsupportedCppShape) -> String {
    format!(
        "error[{}]: structured C++ symbol `{}` (`{}` at {}) cannot be bound",
        skip_reason(shape),
        symbol.id,
        symbol.cpp_name,
        source_location_text(&symbol.source)
    )
}

pub(super) fn collect_skipped_symbols(
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<Vec<SkippedSymbol>, String> {
    let mut skipped = Vec::new();
    for policy in symbols.policies.values() {
        if policy.disposition != CppSymbolDisposition::Reject {
            continue;
        }
        let symbol = symbols
            .declarations
            .get(policy.symbol.as_str())
            .expect("validated C++ declaration");
        let rejection = policy
            .rejection
            .as_ref()
            .expect("validated C++ rejection policy");
        let reason = skip_reason(rejection.shape);
        if ALLOWED_CPP_SKIP_FAMILIES.binary_search(&reason).is_err() {
            return Err(format!(
                "error[cpp.skip_family.unknown]: rejected C++ symbol `{}` mapped to unapproved family `{reason}`",
                symbol.id
            ));
        }
        skipped.push(SkippedSymbol {
            id: symbol.id.clone(),
            symbol: symbol.cpp_name.clone(),
            source: source_location_text(&symbol.source),
            reason: reason.to_string(),
            message: skip_message(rejection.shape).to_string(),
        });
    }
    skipped.sort();
    Ok(skipped)
}

pub(super) fn skip_reason(shape: UnsupportedCppShape) -> &'static str {
    match shape {
        UnsupportedCppShape::RawPointerOwnership => "cpp.pointer.unsupported",
        UnsupportedCppShape::ReferenceLifetimeAmbiguity => "cpp.lifetime.borrowed",
        UnsupportedCppShape::UnsupportedTemplate => "cpp.template.unspecialized",
        UnsupportedCppShape::ExceptionBoundary => "cpp.exception.crossing",
        UnsupportedCppShape::OverloadAmbiguity => "cpp.overload.ambiguous",
        UnsupportedCppShape::UnsupportedMacro => "cpp.annotation.unsupported",
        UnsupportedCppShape::UnsupportedVariadicFunction => "cpp.variadic.unsupported",
        UnsupportedCppShape::UnsupportedInheritance => "cpp.inheritance.unsupported",
        UnsupportedCppShape::UnsupportedCallbackShape => "cpp.callback.unsupported",
        UnsupportedCppShape::UnknownOwnership => "cpp.ownership.unknown",
        UnsupportedCppShape::UnmappedType => "cpp.type.unmapped",
    }
}

pub(super) fn skip_message(shape: UnsupportedCppShape) -> &'static str {
    match shape {
        UnsupportedCppShape::RawPointerOwnership => {
            "Raw pointer ownership is not represented by the generated boundary."
        }
        UnsupportedCppShape::ReferenceLifetimeAmbiguity => {
            "Borrowed reference lifetime is not represented by the generated boundary."
        }
        UnsupportedCppShape::UnsupportedTemplate => {
            "Unspecialized templates do not have a concrete generated ABI."
        }
        UnsupportedCppShape::ExceptionBoundary => {
            "Uncontained C++ exceptions cannot cross the generated boundary."
        }
        UnsupportedCppShape::OverloadAmbiguity => {
            "Ambiguous overloads do not have a unique generated name."
        }
        UnsupportedCppShape::UnsupportedMacro => {
            "Preprocessor macros do not have a typed callable ABI."
        }
        UnsupportedCppShape::UnsupportedVariadicFunction => {
            "Variadic arguments do not have a stable generated signature."
        }
        UnsupportedCppShape::UnsupportedInheritance => {
            "Unsupported inheritance layout cannot cross the generated boundary."
        }
        UnsupportedCppShape::UnsupportedCallbackShape => {
            "Callback lifetime is not represented by the generated boundary."
        }
        UnsupportedCppShape::UnknownOwnership => {
            "Result ownership was not classified by package policy."
        }
        UnsupportedCppShape::UnmappedType => {
            "The canonical C++ type has no reviewed Terlan mapping."
        }
    }
}

pub(super) fn source_location_text(source: &CppSourceLocation) -> String {
    format!("{}:{}:{}", source.path, source.line, source.column)
}

pub(super) fn validate_compile_configuration(
    compile: &CppCompileConfiguration,
    input_dir: &Path,
) -> Result<(), String> {
    if compile.include_roots.is_empty() {
        return Err("structured C++ compile configuration requires include roots".into());
    }
    for root in &compile.include_roots {
        let path = Path::new(root);
        if root.trim().is_empty()
            || path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
            || !input_dir.join(path).is_dir()
        {
            return Err(format!(
                "C++ include root `{root}` must resolve to a package-relative directory"
            ));
        }
    }
    for (name, value) in &compile.defines {
        if !is_identifier_segment(name) {
            return Err(format!("invalid C++ preprocessor define `{name}`"));
        }
        if value
            .as_deref()
            .is_some_and(|value| value.chars().any(|ch| matches!(ch, '\0' | '\n' | '\r')))
        {
            return Err(format!(
                "C++ preprocessor define `{name}` contains an invalid value"
            ));
        }
    }
    if compile.arguments.is_empty()
        || compile.arguments.iter().any(|argument| {
            argument.is_empty() || argument.chars().any(|ch| matches!(ch, '\0' | '\n' | '\r'))
        })
    {
        return Err(
            "structured C++ compile configuration requires non-empty, NUL-free arguments".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_enum_presence_group_is_not_mistaken_for_later_optional_bool() {
        let args: Vec<NativeBindingArg> = serde_json::from_value(serde_json::json!([
            {"name": "has_dtype", "ty": "Bool"},
            {"name": "dtype", "ty": "DType"}
        ]))
        .expect("presence group");
        let parameter: CppParameter = serde_json::from_value(serde_json::json!({
            "name": "dtype",
            "ty": {
                "spelling": "std::optional<ScalarType>",
                "canonical": "std::optional<c10::ScalarType>",
                "is_const": false,
                "pointer_depth": 0,
                "reference": "none",
                "function_pointer": false,
                "template_dependent": false
            },
            "direction": "input"
        }))
        .expect("optional enum parameter");
        let later_parameter: CppParameter = serde_json::from_value(serde_json::json!({
            "name": "pin_memory",
            "ty": {
                "spelling": "std::optional<bool>",
                "canonical": "std::optional<bool>",
                "is_const": false,
                "pointer_depth": 0,
                "reference": "none",
                "function_pointer": false,
                "template_dependent": false
            },
            "direction": "input"
        }))
        .expect("later optional bool parameter");
        assert!(optional_presence_pair(&args, 0, &parameter));
        assert!(!should_omit_default_optional_parameter(
            &args,
            0,
            &[parameter, later_parameter],
            0
        ));
    }
}
