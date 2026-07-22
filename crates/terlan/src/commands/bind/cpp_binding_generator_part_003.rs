
/// Validates the getter set used to construct one copied record result.
fn validate_value_projection(
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
fn validate_owned_value_projection(
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
fn validate_projection_fields(
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
fn owned_unique_ptr_name(ty: &CppTypeMetadata) -> Option<&str> {
    ty.canonical
        .strip_prefix("std::unique_ptr<")
        .and_then(|value| value.strip_suffix('>'))
}

/// Matches one Terlan primitive record field to its extracted C++ getter type.
fn projection_type_matches(field: &str, returns: &CppTypeMetadata) -> bool {
    match field {
        "Int" => is_i64_type(returns),
        "Float" => returns.canonical == "double",
        "Bool" => returns.canonical == "bool",
        _ => false,
    }
}

fn validate_mapping_policy<'a>(
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

fn validate_symbol_policy(symbol: &CppSymbol, policy: &CppSymbolPolicy) -> Result<(), String> {
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

fn validate_bindable_cpp_symbol(
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
    if symbol.overload_candidates > 1 {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCppShape::OverloadAmbiguity,
        ));
    }
    if symbol.variadic {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCppShape::UnsupportedVariadicFunction,
        ));
    }
    if !symbol.inheritance.is_empty() {
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
        validate_cpp_type_shape(symbol, returns)?;
    }
    for parameter in &symbol.parameters {
        validate_cpp_type_shape(symbol, &parameter.ty)?;
    }
    Ok(())
}

fn validate_cpp_type_metadata(
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

fn validate_cpp_type_shape(symbol: &CppSymbol, cpp_type: &CppTypeMetadata) -> Result<(), String> {
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
fn validate_function_argument_mapping(
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), String> {
    if function.role == NativeFunctionRole::MutableMethod
        && function
            .args
            .iter()
            .skip(1)
            .any(|argument| find_resource_type_in_modules(modules, &argument.ty).is_some())
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
    let mut parameter_index = 0;
    for argument in args {
        if argument.fields.is_empty() {
            let parameter = symbol.parameters.get(parameter_index).ok_or_else(|| {
                format!(
                    "error[cpp.type.argument_count]: function `{}` maps more public values than the {} extracted C++ parameters",
                    function.name,
                    symbol.parameters.len()
                )
            })?;
            validate_scalar_argument(function, argument, parameter, modules, symbols)?;
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
    }
    if parameter_index != symbol.parameters.len() {
        return Err(format!(
            "error[cpp.type.argument_count]: function `{}` maps {parameter_index} public scalar values to {} C++ parameters",
            function.name,
            symbol.parameters.len()
        ));
    }
    Ok(())
}

/// Requires one ordinary public argument to match one extracted C++ parameter.
fn validate_scalar_argument(
    function: &NativeBindingFunction,
    argument: &NativeBindingArg,
    parameter: &CppParameter,
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), String> {
    let compatible = terlan_primitive_matches_cpp(&argument.ty, &parameter.ty)
        || matches!(argument.ty.as_str(), "String") && is_rust_str_type(&parameter.ty)
        || matches!(argument.ty.as_str(), "std.vm.Bytes.Bytes" | "Bytes")
            && is_u8_slice_type(&parameter.ty)
        || argument.ty == "List[Int]" && is_i64_slice_type(&parameter.ty)
        || argument.ty == "List[Float]" && is_f64_slice_type(&parameter.ty)
        || modules
            .iter()
            .flat_map(|module| &module.types)
            .any(|binding_type| {
                binding_type.kind == NativeBindingTypeKind::Enum
                    && terlan_type_matches(&argument.ty, &binding_type.name)
                    && is_i64_type(&parameter.ty)
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
                })
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
fn find_resource_type_in_modules<'a>(
    modules: &'a [NativeBindingModule],
    value: &str,
) -> Option<&'a NativeBindingType> {
    modules.iter().flat_map(|module| &module.types).find(|ty| {
        ty.kind == NativeBindingTypeKind::OpaqueResource && terlan_type_matches(value, &ty.name)
    })
}

/// Matches copied Terlan scalar types to supported by-value C++ scalars.
fn terlan_primitive_matches_cpp(ty: &str, cpp_type: &CppTypeMetadata) -> bool {
    match ty {
        "Int" => is_i64_type(cpp_type),
        "Float" => cpp_type.canonical == "double",
        "Bool" => cpp_type.canonical == "bool",
        _ => false,
    }
}

/// Requires each public result type to match the extracted C++ ownership and
/// container shape before any bridge source is emitted.
fn validate_function_return_mapping(
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
    modules: &[NativeBindingModule],
) -> Result<(), String> {
    let returns = symbol.returns.as_ref();
    let canonical = returns
        .map(|returns| returns.canonical.as_str())
        .unwrap_or("void");
    let compatible = match function.returns.as_str() {
        "Unit" => canonical == "void",
        "Int" => returns.is_some_and(is_i64_type),
        "Float" => canonical == "double",
        "Bool" => canonical == "bool",
        "String" => returns.is_some_and(is_owned_string_type),
        "std.vm.Bytes.Bytes" => returns.is_some_and(is_owned_u8_vector_type),
        "List[Int]" => returns.is_some_and(is_owned_i64_vector_type),
        "List[Float]" => returns.is_some_and(is_owned_f64_vector_type),
        returns => modules.iter().flat_map(|module| &module.types).any(|ty| {
            ty.kind == NativeBindingTypeKind::OpaqueResource
                && terlan_type_matches(returns, &ty.name)
                && canonical.starts_with("std::unique_ptr<")
        }),
    };
    if !compatible {
        return Err(format!(
            "error[cpp.type.mapping_mismatch]: function `{}` maps C++ result `{canonical}` to incompatible Terlan type `{}`",
            function.name, function.returns
        ));
    }
    Ok(())
}

fn is_supported_cxx_type(cpp_type: &CppTypeMetadata) -> bool {
    let canonical = cpp_type.canonical.as_str();
    canonical == "void"
        || is_i64_type(cpp_type)
        || canonical == "double"
        || canonical == "bool"
        || is_rust_str_type(cpp_type)
        || is_u8_slice_type(cpp_type)
        || is_i64_slice_type(cpp_type)
        || is_f64_slice_type(cpp_type)
        || borrowed_const_record_name(cpp_type).is_some()
        || canonical
            .strip_prefix("std::unique_ptr<")
            .and_then(|value| value.strip_suffix('>'))
            .is_some_and(|inner| !inner.trim().is_empty())
}

/// Returns a whitespace-insensitive declared C++ type spelling.
fn compact_cpp_spelling(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

/// Returns the named record borrowed through one immutable C++ reference.
fn borrowed_const_record_name(cpp_type: &CppTypeMetadata) -> Option<&str> {
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

/// Matches an extractor-qualified C++ name to its declaration-local name.
fn cpp_name_matches(value: &str, expected: &str) -> bool {
    value == expected || value.ends_with(&format!("::{expected}"))
}

/// Returns the declaration-local segment of one extractor-qualified C++ name.
fn cpp_short_name(value: &str) -> &str {
    value.rsplit("::").next().unwrap_or(value)
}

/// Recognizes a declared fixed-width signed 64-bit alias across Clang targets.
fn is_i64_type(cpp_type: &CppTypeMetadata) -> bool {
    matches!(
        compact_cpp_spelling(&cpp_type.spelling).as_str(),
        "std::int64_t" | "int64_t"
    ) || cpp_type.canonical == "std::int64_t"
}

/// Recognizes the CXX borrowed UTF-8 string argument type.
fn is_rust_str_type(cpp_type: &CppTypeMetadata) -> bool {
    compact_cpp_spelling(&cpp_type.spelling) == "rust::Str" || cpp_type.canonical == "rust::Str"
}

/// Recognizes a CXX borrowed unsigned-byte slice after typedef canonicalization.
fn is_u8_slice_type(cpp_type: &CppTypeMetadata) -> bool {
    matches!(
        compact_cpp_spelling(&cpp_type.spelling).as_str(),
        "rust::Slice<conststd::uint8_t>" | "rust::Slice<constuint8_t>"
    ) || cpp_type.canonical == "rust::Slice<const std::uint8_t>"
}

/// Recognizes a CXX borrowed signed 64-bit slice after typedef canonicalization.
fn is_i64_slice_type(cpp_type: &CppTypeMetadata) -> bool {
    matches!(
        compact_cpp_spelling(&cpp_type.spelling).as_str(),
        "rust::Slice<conststd::int64_t>" | "rust::Slice<constint64_t>"
    ) || cpp_type.canonical == "rust::Slice<const std::int64_t>"
}

/// Recognizes a CXX borrowed double slice.
fn is_f64_slice_type(cpp_type: &CppTypeMetadata) -> bool {
    compact_cpp_spelling(&cpp_type.spelling) == "rust::Slice<constdouble>"
        || cpp_type.canonical == "rust::Slice<const double>"
}

/// Recognizes an owned standard string result from its declared contract.
fn is_owned_string_type(cpp_type: &CppTypeMetadata) -> bool {
    compact_cpp_spelling(&cpp_type.spelling) == "std::unique_ptr<std::string>"
}

/// Recognizes an owned unsigned-byte vector despite canonical typedef expansion.
fn is_owned_u8_vector_type(cpp_type: &CppTypeMetadata) -> bool {
    compact_cpp_spelling(&cpp_type.spelling) == "std::unique_ptr<std::vector<std::uint8_t>>"
}

/// Recognizes an owned signed 64-bit vector despite target-specific aliases.
fn is_owned_i64_vector_type(cpp_type: &CppTypeMetadata) -> bool {
    compact_cpp_spelling(&cpp_type.spelling) == "std::unique_ptr<std::vector<std::int64_t>>"
}

/// Recognizes an owned double vector used for copied floating-point lists.
fn is_owned_f64_vector_type(cpp_type: &CppTypeMetadata) -> bool {
    compact_cpp_spelling(&cpp_type.spelling) == "std::unique_ptr<std::vector<double>>"
}

fn stable_shape_error(symbol: &CppSymbol, shape: UnsupportedCppShape) -> String {
    format!(
        "error[{}]: structured C++ symbol `{}` (`{}` at {}) cannot be bound",
        skip_reason(shape),
        symbol.id,
        symbol.cpp_name,
        source_location_text(&symbol.source)
    )
}

fn collect_skipped_symbols(
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
        skipped.push(SkippedSymbol {
            id: symbol.id.clone(),
            symbol: symbol.cpp_name.clone(),
            source: source_location_text(&symbol.source),
            reason: skip_reason(rejection.shape).to_string(),
            detail: rejection.detail.clone(),
        });
    }
    skipped.sort();
    Ok(skipped)
}

fn skip_reason(shape: UnsupportedCppShape) -> &'static str {
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

fn source_location_text(source: &CppSourceLocation) -> String {
    format!("{}:{}:{}", source.path, source.line, source.column)
}

fn validate_compile_configuration(
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
