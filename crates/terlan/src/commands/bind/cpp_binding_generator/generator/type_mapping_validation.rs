use super::*;

/// Validates the getter set used to construct one copied record result.
pub(super) fn validate_value_projection(
    function: &NativeBindingFunction,
    module: &NativeBindingModule,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), CppBindingError> {
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
        return Err((format!(
            "value projection `{}` currently requires exactly one resource argument",
            function.name
        ))
        .into());
    }
    validate_projection_fields(function, record, resource_symbol, symbols)
}

/// Validates a temporary owned C++ value projected directly into a Terlan record.
pub(super) fn validate_owned_value_projection(
    function: &NativeBindingFunction,
    producer: &CppSymbol,
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), CppBindingError> {
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
        return Err((format!(
            "owned value projection `{}` requires a free function returning std::unique_ptr<{}>",
            function.name, record_symbol.cpp_name
        ))
        .into());
    }
    if function.resource != NativeResourcePolicy::Value {
        return Err((format!(
            "owned value projection `{}` must expose a copied value result",
            function.name
        ))
        .into());
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
) -> Result<(), CppBindingError> {
    if function.projections.len() != record.fields.len() {
        return Err((format!(
            "value projection `{}` requires exactly {} field projections",
            function.name,
            record.fields.len()
        ))
        .into());
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
            return Err((format!(
                "value projection `{}` duplicates field `{}`",
                function.name, field.name
            ))
            .into());
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
            return Err((format!(
                "value projection `{}.{}` requires a bindable zero-argument {} getter",
                function.name, field.name, field.ty
            ))
            .into());
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
) -> Result<BTreeMap<&'a str, &'a CppSymbolPolicy>, CppBindingError> {
    let mut policies = BTreeMap::new();
    for policy in &mapping.symbols {
        let symbol = declarations.get(policy.symbol.as_str()).ok_or_else(|| {
            format!(
                "C++ mapping policy references unknown extracted symbol `{}`",
                policy.symbol
            )
        })?;
        if policies.insert(policy.symbol.as_str(), policy).is_some() {
            return Err((format!(
                "duplicate C++ mapping policy for symbol `{}`",
                policy.symbol
            ))
            .into());
        }
        validate_symbol_policy(symbol, policy)?;
    }
    for symbol in declarations.keys() {
        if !policies.contains_key(symbol) {
            return Err((format!(
                "error[cpp.mapping.missing]: extracted C++ symbol `{symbol}` has no package-owned mapping policy"
            )).into());
        }
    }
    Ok(policies)
}

pub(super) fn validate_symbol_policy(
    symbol: &CppSymbol,
    policy: &CppSymbolPolicy,
) -> Result<(), CppBindingError> {
    match policy.disposition {
        CppSymbolDisposition::Reject => {
            let rejection = policy.rejection.as_ref().ok_or_else(|| {
                format!(
                    "rejected C++ symbol `{}` requires a stable rejection policy",
                    symbol.id
                )
            })?;
            if rejection.detail.trim().is_empty() {
                return Err((format!(
                    "rejected C++ symbol `{}` requires stable rejection detail",
                    symbol.id
                ))
                .into());
            }
            if policy.ownership.is_some()
                || policy.thread_safety.is_some()
                || policy.exception.is_some()
            {
                return Err((format!(
                    "rejected C++ symbol `{}` cannot carry binding policy",
                    symbol.id
                ))
                .into());
            }
        }
        CppSymbolDisposition::Bind => {
            if policy.rejection.is_some() {
                return Err((format!(
                    "bindable C++ symbol `{}` cannot carry a rejection policy",
                    symbol.id
                ))
                .into());
            }
            if symbol.kind == CppSymbolKind::Record
                && (policy.ownership.is_none() || policy.thread_safety.is_none())
            {
                return Err(
                    (stable_shape_error(symbol, UnsupportedCppShape::UnknownOwnership)).into(),
                );
            }
            if matches!(symbol.kind, CppSymbolKind::Record | CppSymbolKind::Enum)
                && policy.exception.is_some()
            {
                return Err((format!(
                    "C++ type symbol `{}` cannot declare callable exception policy",
                    symbol.id
                ))
                .into());
            }
            if let Some(exception) = &policy.exception {
                validate_lower_identifier("exception error code", &exception.error_code)?;
                if exception.message.trim().is_empty()
                    || exception
                        .message
                        .chars()
                        .any(|ch| matches!(ch, '\0' | '\n' | '\r'))
                {
                    return Err((format!(
                        "C++ exception policy for `{}` requires a stable one-line message",
                        symbol.id
                    ))
                    .into());
                }
            }
            if symbol.noexcept && policy.exception.is_some() {
                return Err((format!(
                    "noexcept C++ symbol `{}` cannot declare exception containment policy",
                    symbol.id
                ))
                .into());
            }
            validate_bindable_cpp_symbol(symbol, policy.exception.is_some())?;
        }
    }
    Ok(())
}

pub(super) fn validate_bindable_cpp_symbol(
    symbol: &CppSymbol,
    exception_contained: bool,
) -> Result<(), CppBindingError> {
    if symbol.kind == CppSymbolKind::Macro {
        return Err((stable_shape_error(symbol, UnsupportedCppShape::UnsupportedMacro)).into());
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
        return Err((stable_shape_error(symbol, UnsupportedCppShape::UnsupportedTemplate)).into());
    }
    if symbol.variadic {
        return Err(
            (stable_shape_error(symbol, UnsupportedCppShape::UnsupportedVariadicFunction)).into(),
        );
    }
    if symbol.kind != CppSymbolKind::Record && !symbol.inheritance.is_empty() {
        return Err(
            (stable_shape_error(symbol, UnsupportedCppShape::UnsupportedInheritance)).into(),
        );
    }
    if !matches!(symbol.kind, CppSymbolKind::Record | CppSymbolKind::Enum)
        && !symbol.noexcept
        && !exception_contained
    {
        return Err((stable_shape_error(symbol, UnsupportedCppShape::ExceptionBoundary)).into());
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
) -> Result<(), CppBindingError> {
    if cpp_type.spelling.trim().is_empty() || cpp_type.canonical.trim().is_empty() {
        return Err((format!(
            "structured C++ type metadata for `{}::{position}` requires declared and canonical spellings",
            symbol.id
        )).into());
    }
    Ok(())
}

pub(super) fn validate_cpp_type_shape(
    symbol: &CppSymbol,
    cpp_type: &CppTypeMetadata,
) -> Result<(), CppBindingError> {
    if cpp_type.function_pointer {
        return Err(
            (stable_shape_error(symbol, UnsupportedCppShape::UnsupportedCallbackShape)).into(),
        );
    }
    if cpp_type.pointer_depth > 0 {
        return Err((stable_shape_error(symbol, UnsupportedCppShape::RawPointerOwnership)).into());
    }
    if cpp_type.reference != CppReferenceKind::None
        && borrowed_const_record_name(cpp_type).is_none()
        && cpp_array_ref_element(cpp_type).is_none()
    {
        return Err(
            (stable_shape_error(symbol, UnsupportedCppShape::ReferenceLifetimeAmbiguity)).into(),
        );
    }
    if !cpp_type.enum_type && !is_supported_cxx_type(cpp_type) {
        return Err((stable_shape_error(symbol, UnsupportedCppShape::UnmappedType)).into());
    }
    Ok(())
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
) -> Result<Vec<SkippedSymbol>, CppBindingError> {
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
            return Err((format!(
                "error[cpp.skip_family.unknown]: rejected C++ symbol `{}` mapped to unapproved family `{reason}`",
                symbol.id
            )).into());
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
) -> Result<(), CppBindingError> {
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
            return Err((format!(
                "C++ include root `{root}` must resolve to a package-relative directory"
            ))
            .into());
        }
    }
    for (name, value) in &compile.defines {
        if !is_identifier_segment(name) {
            return Err((format!("invalid C++ preprocessor define `{name}`")).into());
        }
        if value
            .as_deref()
            .is_some_and(|value| value.chars().any(|ch| matches!(ch, '\0' | '\n' | '\r')))
        {
            return Err(
                (format!("C++ preprocessor define `{name}` contains an invalid value")).into(),
            );
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
#[path = "type_mapping_validation_test.rs"]
mod tests;
