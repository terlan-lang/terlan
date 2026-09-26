use super::*;

/// Validates a package-owned null-result classifier against extracted C++ facts.
pub(super) fn validate_null_failure_policy(
    manifest: &NativeBindingManifest,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), CppBindingError> {
    let Some(policy) = &manifest.null_failure else {
        return Ok(());
    };
    if policy.cases.is_empty() {
        return Err("C++ null failure policy requires at least one finite case".into());
    }
    let probe = symbols
        .declarations
        .get(policy.probe_symbol.as_str())
        .ok_or_else(|| {
            format!(
                "C++ null failure probe references unknown symbol `{}`",
                policy.probe_symbol
            )
        })?;
    if !symbols.is_bindable(&policy.probe_symbol)
        || probe.kind != CppSymbolKind::Function
        || !probe.parameters.is_empty()
        || !probe.noexcept
        || !probe.returns.as_ref().is_some_and(is_i64_type)
    {
        return Err((format!(
            "C++ null failure probe `{}` must be a bindable no-argument noexcept Int function",
            policy.probe_symbol
        ))
        .into());
    }
    if manifest.modules.iter().any(|module| {
        module
            .functions
            .iter()
            .any(|function| function.cpp_symbol.as_deref() == Some(policy.probe_symbol.as_str()))
    }) {
        return Err((format!(
            "C++ null failure probe `{}` must remain hidden from Terlan modules",
            policy.probe_symbol
        ))
        .into());
    }

    let mut values = BTreeSet::new();
    let mut codes = BTreeSet::new();
    for case in &policy.cases {
        if !values.insert(case.value) {
            return Err(
                (format!("duplicate C++ null failure status value `{}`", case.value)).into(),
            );
        }
        validate_stable_failure("C++ null failure case", &case.failure)?;
        if !codes.insert(case.failure.code.as_str()) {
            return Err((format!(
                "duplicate C++ null failure error code `{}`",
                case.failure.code
            ))
            .into());
        }
    }
    validate_stable_failure("C++ null failure fallback", &policy.fallback)
}

/// Validates one finite package error without trusting native diagnostics.
pub(super) fn validate_stable_failure(
    kind: &str,
    failure: &CppStableFailure,
) -> Result<(), CppBindingError> {
    if failure.code.is_empty()
        || failure.code.len() > 128
        || !failure.code.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        })
    {
        return Err((format!("{kind} has an invalid package error code")).into());
    }
    if failure.message.trim().is_empty()
        || failure.message.len() > 4096
        || failure.message.chars().any(char::is_control)
    {
        return Err((format!("{kind} requires a stable single-line message")).into());
    }
    Ok(())
}

pub(super) fn validate_cpp_symbol(symbol: &CppSymbol) -> Result<(), CppBindingError> {
    if symbol.id.trim().is_empty() || symbol.cpp_name.trim().is_empty() {
        return Err("structured C++ symbols require stable id and cpp_name fields".into());
    }
    if symbol.source.path.trim().is_empty() || symbol.source.line == 0 || symbol.source.column == 0
    {
        return Err((format!(
            "structured C++ symbol `{}` requires a non-empty, one-based source location",
            symbol.id
        ))
        .into());
    }
    if symbol.documentation.trim().is_empty() {
        return Err((format!(
            "structured C++ symbol `{}` requires extracted documentation",
            symbol.id
        ))
        .into());
    }
    if symbol.overload_set.trim().is_empty() {
        return Err((format!(
            "structured C++ symbol `{}` requires stable overload-set identity",
            symbol.id
        ))
        .into());
    }
    if symbol
        .annotations
        .iter()
        .any(|annotation| annotation.trim().is_empty())
    {
        return Err((format!(
            "structured C++ symbol `{}` contains an empty annotation",
            symbol.id
        ))
        .into());
    }
    if let Some(returns) = &symbol.returns {
        validate_cpp_type_metadata(symbol, "return", returns)?;
    }
    for parameter in &symbol.parameters {
        // Parameter spellings are extracted facts, not package-authored Rust
        // identifiers. Real C++ APIs legitimately use leading acronyms such
        // as LibTorch's `LU_data` and `LU_pivots`.
        validate_cpp_identifier("C++ parameter", &parameter.name)?;
        validate_cpp_type_metadata(symbol, &parameter.name, &parameter.ty)?;
        if parameter.direction != CppParameterDirection::Input
            && parameter.ty.pointer_depth == 0
            && parameter.ty.reference == CppReferenceKind::None
        {
            return Err((format!(
                "C++ output parameter `{}::{}` requires pointer or reference type facts",
                symbol.id, parameter.name
            ))
            .into());
        }
        if parameter.default.as_deref().is_some_and(|value| {
            let value = value.trim();
            value.is_empty()
                || value
                    .strip_prefix('=')
                    .is_some_and(|inner| inner.trim().is_empty())
        }) {
            return Err((format!(
                "C++ parameter `{}::{}` has an empty default expression",
                symbol.id, parameter.name
            ))
            .into());
        }
    }
    let mut field_names = BTreeSet::new();
    for field in &symbol.fields {
        validate_lower_identifier("C++ record field", &field.name)?;
        validate_cpp_type_metadata(symbol, &field.name, &field.ty)?;
        if !field_names.insert(field.name.as_str()) {
            return Err((format!(
                "structured C++ record `{}` contains duplicate field `{}`",
                symbol.id, field.name
            ))
            .into());
        }
    }
    if symbol.kind != CppSymbolKind::Record && !symbol.fields.is_empty() {
        return Err((format!(
            "non-record C++ symbol `{}` cannot contain record fields",
            symbol.id
        ))
        .into());
    }
    if symbol.kind != CppSymbolKind::Enum && !symbol.enum_values.is_empty() {
        return Err((format!(
            "non-enum C++ symbol `{}` cannot contain enumerators",
            symbol.id
        ))
        .into());
    }
    if symbol.kind == CppSymbolKind::Enum {
        validate_cpp_enum(symbol)?;
    }
    if symbol.kind == CppSymbolKind::Method && symbol.receiver.is_none() {
        return Err((format!("C++ method `{}` requires receiver metadata", symbol.id)).into());
    }
    Ok(())
}

/// Validates extractor-owned C++ enumerator names and discriminant spellings.
pub(super) fn validate_cpp_enum(symbol: &CppSymbol) -> Result<(), CppBindingError> {
    if symbol.enum_values.is_empty() {
        return Err((format!(
            "structured C++ enum `{}` requires at least one enumerator",
            symbol.id
        ))
        .into());
    }
    let mut names = BTreeSet::new();
    for value in &symbol.enum_values {
        if !is_identifier_segment(&value.name) || !names.insert(value.name.as_str()) {
            return Err((format!(
                "structured C++ enum `{}` has invalid or duplicate enumerator `{}`",
                symbol.id, value.name
            ))
            .into());
        }
        if value.value.is_empty()
            || !value
                .value
                .chars()
                .enumerate()
                .all(|(index, ch)| ch.is_ascii_digit() || (index == 0 && ch == '-'))
        {
            return Err((format!(
                "structured C++ enum `{}::{}` has invalid discriminant `{}`",
                symbol.id, value.name, value.value
            ))
            .into());
        }
    }
    Ok(())
}

/// Validates a complete one-to-one copied field mapping.
pub(super) fn validate_value_record(
    ty: &NativeBindingType,
    symbol: &CppSymbol,
) -> Result<(), CppBindingError> {
    if ty.fields.is_empty() {
        return Err((format!(
            "value record `{}` requires at least one copied field",
            ty.name
        ))
        .into());
    }
    let cpp_fields = symbol
        .fields
        .iter()
        .filter(|field| {
            matches!(
                field.access,
                CppMemberAccess::Public | CppMemberAccess::None
            )
        })
        .map(|field| (field.name.as_str(), field))
        .collect::<BTreeMap<_, _>>();
    let mut mapped_cpp_fields = BTreeSet::new();
    let mut terlan_fields = BTreeSet::new();
    for field in &ty.fields {
        validate_lower_identifier("value record field", &field.name)?;
        if !terlan_fields.insert(field.name.as_str()) {
            return Err((format!(
                "value record `{}` contains duplicate field `{}`",
                ty.name, field.name
            ))
            .into());
        }
        let cpp_field = cpp_fields.get(field.cpp_field.as_str()).ok_or_else(|| {
            format!(
                "value record `{}.{}` references unknown C++ field `{}`",
                ty.name, field.name, field.cpp_field
            )
        })?;
        if !mapped_cpp_fields.insert(field.cpp_field.as_str()) {
            return Err((format!(
                "C++ field `{}` is mapped more than once by value record `{}`",
                field.cpp_field, ty.name
            ))
            .into());
        }
        if !terlan_primitive_matches_cpp(&field.ty, &cpp_field.ty) {
            return Err((format!(
                "value record `{}.{}` maps incompatible Terlan type `{}` to C++ field `{}`",
                ty.name, field.name, field.ty, cpp_field.ty.canonical
            ))
            .into());
        }
    }
    if mapped_cpp_fields.len() != cpp_fields.len() {
        return Err((format!(
            "value record `{}` must map all {} extracted C++ fields",
            ty.name,
            cpp_fields.len()
        ))
        .into());
    }
    Ok(())
}

/// Validates a curated symbolic subset of one extractor-owned C++ enum.
pub(super) fn validate_enum_mapping(
    ty: &NativeBindingType,
    symbol: &CppSymbol,
) -> Result<(), CppBindingError> {
    if ty.variants.is_empty() {
        return Err((format!(
            "enum `{}` requires at least one reviewed symbolic variant",
            ty.name
        ))
        .into());
    }
    let extracted = symbol
        .enum_values
        .iter()
        .map(|value| value.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut names = BTreeSet::new();
    let mut cpp_names = BTreeSet::new();
    let mut atoms = BTreeSet::new();
    for variant in &ty.variants {
        validate_upper_identifier("enum variant", &variant.name)?;
        if !extracted.contains(variant.cpp_name.as_str()) {
            return Err((format!(
                "enum `{}.{}` references unknown C++ enumerator `{}`",
                ty.name, variant.name, variant.cpp_name
            ))
            .into());
        }
        if !names.insert(variant.name.as_str())
            || !cpp_names.insert(variant.cpp_name.as_str())
            || !atoms.insert(variant.atom.as_str())
        {
            return Err((format!(
                "enum `{}` contains duplicate public names, C++ enumerators, or atoms",
                ty.name
            ))
            .into());
        }
        validate_lower_identifier("enum atom", &variant.atom)?;
        if variant.documentation.trim().is_empty() {
            return Err((format!(
                "enum variant `{}.{}` documentation cannot be empty",
                ty.name, variant.name
            ))
            .into());
        }
    }
    Ok(())
}

/// Validates one immutable resource method converted through a symbolic enum adapter.
pub(super) fn validate_enum_projection(
    function: &NativeBindingFunction,
    module: &NativeBindingModule,
    symbol: &CppSymbol,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), CppBindingError> {
    let enum_type = module
        .types
        .iter()
        .find(|ty| {
            ty.kind == NativeBindingTypeKind::Enum
                && terlan_type_matches(&function.returns, &ty.name)
        })
        .ok_or_else(|| {
            format!(
                "enum projection `{}` must return a module-owned enum",
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
                "enum projection `{}` requires an opaque resource argument",
                function.name
            )
        })?;
    let resource_symbol = symbols
        .declarations
        .get(resource.cpp_symbol.as_str())
        .expect("validated enum projection resource");
    let enum_symbol = symbols
        .declarations
        .get(enum_type.cpp_symbol.as_str())
        .expect("validated enum projection result");
    let returns = symbol.returns.as_ref();
    if function.args.len() != 1
        || symbol.kind != CppSymbolKind::Method
        || symbol.receiver_mutable
        || !symbol
            .receiver
            .as_deref()
            .is_some_and(|receiver| cpp_record_supports_receiver(resource_symbol, receiver))
        || symbol
            .parameters
            .iter()
            .any(|parameter| parameter.default.is_none())
        || !returns.is_some_and(|returns| {
            returns.enum_type && cpp_type_matches_symbol(&returns.canonical, enum_symbol)
        })
    {
        return Err((format!(
            "enum projection `{}` requires a bindable zero-argument const enum getter",
            function.name
        ))
        .into());
    }
    Ok(())
}

/// Validates one immutable resource getter converted through a copied value's
/// extracted `std::string` method.
pub(super) fn validate_string_projection(
    function: &NativeBindingFunction,
    module: &NativeBindingModule,
    getter: &CppSymbol,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), CppBindingError> {
    let value_type = module
        .types
        .iter()
        .find(|ty| {
            ty.kind == NativeBindingTypeKind::StringValue
                && terlan_type_matches(&function.returns, &ty.name)
        })
        .ok_or_else(|| {
            format!(
                "string projection `{}` must return a module-owned string value",
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
                "string projection `{}` requires an opaque resource argument",
                function.name
            )
        })?;
    let resource_symbol = symbols
        .declarations
        .get(resource.cpp_symbol.as_str())
        .expect("validated string projection resource");
    let value_symbol = symbols
        .declarations
        .get(value_type.cpp_symbol.as_str())
        .expect("validated string projection value");
    let stringifier_id = value_type.stringifier.as_deref().ok_or_else(|| {
        format!(
            "string value `{}` requires a stringifier for projection `{}`",
            value_type.name, function.name
        )
    })?;
    let stringifier = symbols.declarations.get(stringifier_id).ok_or_else(|| {
        format!(
            "string value `{}` references unknown C++ stringifier `{stringifier_id}`",
            value_type.name
        )
    })?;
    if !symbols.is_bindable(stringifier_id) {
        return Err((format!(
            "string value `{}` references rejected C++ stringifier `{stringifier_id}`",
            value_type.name
        ))
        .into());
    }
    if function.args.len() != 1
        || getter.kind != CppSymbolKind::Method
        || getter.receiver_mutable
        || !getter
            .receiver
            .as_deref()
            .is_some_and(|receiver| cpp_record_supports_receiver(resource_symbol, receiver))
        || !getter.parameters.is_empty()
        || !getter
            .returns
            .as_ref()
            .is_some_and(|returns| cpp_value_type_matches_symbol(returns, value_symbol))
    {
        return Err((format!(
            "string projection `{}` requires a bindable zero-argument const value getter",
            function.name
        ))
        .into());
    }
    if stringifier.kind != CppSymbolKind::Method
        || stringifier.receiver_mutable
        || !stringifier
            .receiver
            .as_deref()
            .is_some_and(|receiver| cpp_record_supports_receiver(value_symbol, receiver))
        || !stringifier.parameters.is_empty()
        || !stringifier
            .returns
            .as_ref()
            .is_some_and(is_std_string_value)
    {
        return Err((format!(
            "string value `{}` requires a bindable zero-argument const std::string stringifier",
            value_type.name
        ))
        .into());
    }
    Ok(())
}

pub(super) fn cpp_value_type_matches_symbol(
    cpp_type: &CppTypeMetadata,
    symbol: &CppSymbol,
) -> bool {
    if cpp_type.pointer_depth != 0
        || cpp_type.function_pointer
        || cpp_type.enum_type
        || !matches!(
            cpp_type.reference,
            CppReferenceKind::None | CppReferenceKind::Lvalue
        )
        || (cpp_type.reference == CppReferenceKind::Lvalue && !cpp_type.is_const)
    {
        return false;
    }
    let canonical = cpp_type
        .canonical
        .trim()
        .strip_prefix("const ")
        .unwrap_or(cpp_type.canonical.trim())
        .trim_end_matches('&')
        .trim();
    cpp_type_matches_symbol(canonical, symbol)
}

pub(super) fn is_std_string_value(cpp_type: &CppTypeMetadata) -> bool {
    if cpp_type.pointer_depth != 0
        || cpp_type.reference != CppReferenceKind::None
        || cpp_type.function_pointer
        || cpp_type.template_dependent
    {
        return false;
    }
    let spelling = compact_cpp_spelling(&cpp_type.spelling);
    let canonical = compact_cpp_spelling(&cpp_type.canonical);
    spelling == "std::string"
        || canonical == "std::string"
        || canonical.starts_with("std::basic_string<char")
}

/// Validates a throwing free function that mutates one or more retained owned
/// resources and returns either `void`, a target alias, or an ordered tuple of
/// aliases to all mutable targets.
pub(super) fn validate_mutable_free_function(
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
    policy: &CppSymbolPolicy,
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), CppBindingError> {
    if symbol.kind != CppSymbolKind::Function
        || symbol.noexcept
        || policy.exception.is_none()
        || function.resource != NativeResourcePolicy::MutableHandle
        || function.returns != "Unit"
        || function.fallible.is_some()
    {
        return Err((format!(
            "mutable free function `{}` requires a throwing C++ free function, stable containment policy, mutable_handle ownership, and Unit result",
            function.name
        )).into());
    }
    let argument = function.args.first().ok_or_else(|| {
        format!(
            "mutable free function `{}` requires its mutation target first",
            function.name
        )
    })?;
    let parameter = symbol.parameters.first().ok_or_else(|| {
        format!(
            "mutable free function `{}` requires its C++ mutation target first",
            function.name
        )
    })?;
    let resource = modules
        .iter()
        .flat_map(|module| &module.types)
        .find(|ty| {
            ty.kind == NativeBindingTypeKind::OpaqueResource
                && terlan_type_matches(&argument.ty, &ty.name)
        })
        .ok_or_else(|| {
            format!(
                "mutable free function `{}` requires an opaque resource target",
                function.name
            )
        })?;
    let resource_symbol = symbols
        .declarations
        .get(resource.cpp_symbol.as_str())
        .expect("validated mutable resource declaration");
    let mutable_target = mutable_record_reference_name(&parameter.ty);
    let logical_const_target = borrowed_const_record_name(&parameter.ty);
    let target_matches = mutable_target.or(logical_const_target).is_some_and(|name| {
        cpp_name_matches(name, &resource_symbol.cpp_name)
            || cpp_name_matches(name, &resource_symbol.overload_set)
    });
    if !target_matches
        || parameter.direction == CppParameterDirection::Input && logical_const_target.is_none()
    {
        return Err((format!(
            "mutable free function `{}` requires its first parameter to be the primary mutable resource reference",
            function.name
        )).into());
    }
    validate_function_argument_mapping(function, symbol, modules, symbols)?;
    let mappings = public_cpp_parameter_mappings(function, symbol);
    for (parameter_index, parameter) in symbol.parameters.iter().enumerate().skip(1) {
        let mapped_argument = mappings
            .iter()
            .find(|mapping| mapping.cpp_parameter_index == parameter_index)
            .map(|mapping| &function.args[mapping.public_argument_index]);
        let public_mutable = mapped_argument.is_some_and(|argument| argument.mutable);
        if (parameter.direction != CppParameterDirection::Input) != public_mutable {
            return Err((format!(
                "mutable free function `{}` must explicitly mark every secondary C++ output parameter as mutable",
                function.name
            )).into());
        }
        if public_mutable {
            let argument = mapped_argument.expect("known mutable argument");
            let resource =
                find_resource_type_in_modules(modules, &argument.ty).ok_or_else(|| {
                    format!(
                    "mutable free function `{}` secondary target `{}` is not an opaque resource",
                    function.name, argument.name
                )
                })?;
            let resource_symbol = symbols
                .declarations
                .get(resource.cpp_symbol.as_str())
                .expect("validated secondary resource declaration");
            if !mutable_record_reference_name(&parameter.ty).is_some_and(|name| {
                cpp_name_matches(name, &resource_symbol.cpp_name)
                    || cpp_name_matches(name, &resource_symbol.overload_set)
            }) {
                return Err((format!(
                    "mutable free function `{}` secondary target `{}` does not map to its mutable C++ resource reference",
                    function.name, argument.name
                )).into());
            }
        }
    }
    let return_matches = symbol.returns.as_ref().is_some_and(|returns| {
        returns.canonical == "void"
            || mutable_record_reference_name(returns)
                .or_else(|| borrowed_const_record_name(returns))
                .is_some_and(|name| {
                    cpp_name_matches(name, &resource_symbol.cpp_name)
                        || cpp_name_matches(name, &resource_symbol.overload_set)
                })
            || mutable_resource_tuple_return_matches(function, symbol, modules, symbols)
    });
    if !return_matches {
        return Err((format!(
            "mutable free function `{}` must return void or ordered resource aliases to its targets",
            function.name
        )).into());
    }
    validate_function_return_mapping(function, symbol, modules, symbols)
}

/// Matches canonical qualified and declaration-local C++ type spellings.
pub(super) fn cpp_type_matches_symbol(canonical: &str, symbol: &CppSymbol) -> bool {
    canonical == symbol.cpp_name
        || canonical == symbol.overload_set
        || canonical.ends_with(&format!("::{}", symbol.cpp_name))
}

/// Validates one throwing resource method and its public `Result` contract.
pub(super) fn validate_exception_method(
    function: &NativeBindingFunction,
    module: &NativeBindingModule,
    symbol: &CppSymbol,
    policy: &CppSymbolPolicy,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), CppBindingError> {
    let fallible = function.fallible.as_ref().ok_or_else(|| {
        format!(
            "exception method `{}` requires typed fallible result metadata",
            function.name
        )
    })?;
    if fallible.ok != "Int" || fallible.error != "std.core.Error.Error" {
        return Err((format!(
            "exception method `{}` currently requires Int success and std.core.Error.Error failure types",
            function.name
        )).into());
    }
    let expected_result = format!("Result[{}, {}]", fallible.ok, fallible.error);
    if function.returns != expected_result {
        return Err((format!(
            "exception method `{}` return type must be `{expected_result}`",
            function.name
        ))
        .into());
    }
    if policy.exception.is_none() || symbol.noexcept {
        return Err((format!(
            "exception method `{}` requires a throwing C++ symbol with containment policy",
            function.name
        ))
        .into());
    }
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
                "exception method `{}` requires an opaque resource argument",
                function.name
            )
        })?;
    let resource_symbol = symbols
        .declarations
        .get(resource.cpp_symbol.as_str())
        .expect("validated exception resource symbol");
    if symbol.kind != CppSymbolKind::Method
        || !symbol
            .receiver
            .as_deref()
            .is_some_and(|receiver| cpp_name_matches(receiver, &resource_symbol.cpp_name))
        || symbol.receiver_mutable
        || !symbol.returns.as_ref().is_some_and(is_i64_type)
        || function.args.len() != symbol.parameters.len() + 1
        || function.args.iter().skip(1).any(|arg| arg.ty != "Int")
        || symbol
            .parameters
            .iter()
            .any(|parameter| !is_i64_type(&parameter.ty))
    {
        return Err((format!(
            "exception method `{}` currently requires a const resource method with Int arguments and result",
            function.name
        )).into());
    }
    Ok(())
}

/// Validates one contained immutable integer getter, including inherited
/// methods proven reachable through public Clang inheritance metadata.
pub(super) fn validate_scalar_projection(
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
    policy: &CppSymbolPolicy,
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), CppBindingError> {
    let return_matches = match function.returns.as_str() {
        "Int" => symbol.returns.as_ref().is_some_and(|returns| {
            is_i64_type(returns)
                || returns.enum_type
                    && symbols.declarations.values().any(|candidate| {
                        candidate.kind == CppSymbolKind::Enum
                            && cpp_type_matches_symbol(&returns.canonical, candidate)
                    })
                || matches!(
                    returns.canonical.as_str(),
                    "signed char" | "short" | "int" | "long" | "long long"
                )
        }),
        "Float" => symbol
            .returns
            .as_ref()
            .is_some_and(|returns| returns.canonical == "double"),
        "Bool" => symbol
            .returns
            .as_ref()
            .is_some_and(|returns| returns.canonical == "bool"),
        _ => false,
    };
    if !return_matches || function.fallible.is_some() {
        return Err((format!(
            "scalar projection `{}` must map an exact Int, Float, or Bool result without a fallible declaration",
            function.name
        )).into());
    }
    if policy.exception.is_none() || symbol.noexcept {
        return Err((format!(
            "scalar projection `{}` requires a throwing C++ symbol with containment policy",
            function.name
        ))
        .into());
    }
    if symbol.kind == CppSymbolKind::Method {
        let resource = function
            .args
            .first()
            .and_then(|arg| find_resource_type_in_modules(modules, &arg.ty))
            .ok_or_else(|| {
                format!(
                    "scalar projection `{}` requires an opaque resource receiver",
                    function.name
                )
            })?;
        let resource_symbol = symbols
            .declarations
            .get(resource.cpp_symbol.as_str())
            .expect("validated scalar projection resource");
        if symbol.receiver_mutable
            || !symbol
                .receiver
                .as_deref()
                .is_some_and(|receiver| cpp_record_supports_receiver(resource_symbol, receiver))
        {
            return Err((format!(
                "scalar projection `{}` requires a const method on its public resource receiver",
                function.name
            ))
            .into());
        }
    } else if symbol.kind != CppSymbolKind::Function {
        return Err((format!(
            "scalar projection `{}` requires a C++ function or const method",
            function.name
        ))
        .into());
    }
    validate_function_argument_mapping(function, symbol, modules, symbols)
}

/// Validates one borrowed integer collection that is copied before returning
/// from its generated C++ adapter.
pub(super) fn validate_int_list_projection(
    function: &NativeBindingFunction,
    module: &NativeBindingModule,
    symbol: &CppSymbol,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), CppBindingError> {
    if function.returns != "List[Int]" || function.fallible.is_some() {
        return Err((format!(
            "integer-list projection `{}` must return List[Int]",
            function.name
        ))
        .into());
    }
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
                "integer-list projection `{}` requires an opaque resource argument",
                function.name
            )
        })?;
    let resource_symbol = symbols
        .declarations
        .get(resource.cpp_symbol.as_str())
        .expect("validated integer-list projection resource");
    if function.args.len() != 1
        || symbol.kind != CppSymbolKind::Method
        || symbol.receiver_mutable
        || !symbol
            .receiver
            .as_deref()
            .is_some_and(|receiver| cpp_record_supports_receiver(resource_symbol, receiver))
        || !symbol.parameters.is_empty()
        || !symbol.returns.as_ref().is_some_and(is_i64_array_ref_type)
    {
        return Err((format!(
            "integer-list projection `{}` requires a bindable zero-argument const IntArrayRef getter",
            function.name
        )).into());
    }
    Ok(())
}
