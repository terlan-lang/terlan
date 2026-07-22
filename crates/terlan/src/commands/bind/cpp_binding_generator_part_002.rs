
fn validate_manifest<'a>(
    manifest: &'a NativeBindingManifest,
    input_dir: &Path,
) -> Result<ValidatedCppSymbols<'a>, String> {
    if manifest.schema != NATIVE_BINDING_SCHEMA {
        return Err(format!(
            "unsupported native binding schema `{}`; expected `{NATIVE_BINDING_SCHEMA}`",
            manifest.schema
        ));
    }
    if manifest.package.adapter != "cxx" {
        return Err(format!(
            "unsupported native binding adapter `{}`; expected `cxx`",
            manifest.package.adapter
        ));
    }
    if manifest.mapping.schema != CPP_MAPPING_SCHEMA {
        return Err(format!(
            "unsupported C++ mapping schema `{}`; expected `{CPP_MAPPING_SCHEMA}`",
            manifest.mapping.schema
        ));
    }
    validate_identifier_path("package namespace", &manifest.package.namespace)?;
    if manifest.package.namespace.starts_with("std.native") {
        return Err("generated external packages cannot use the `std.native` namespace".into());
    }
    validate_cargo_package_name(&manifest.package.crate_name)?;
    validate_build_plan(&manifest.build, input_dir)?;
    let cpp = &manifest.cpp_metadata;
    if cpp.schema != CPP_METADATA_SCHEMA {
        return Err(format!(
            "unsupported structured C++ metadata schema `{}`; expected `{CPP_METADATA_SCHEMA}`",
            cpp.schema
        ));
    }
    if cpp.producer.name != "clang-libtooling" {
        return Err(format!(
            "unsupported C++ metadata producer `{}`; expected maintained tooling `clang-libtooling`",
            cpp.producer.name
        ));
    }
    if cpp.producer.version.trim().is_empty() || cpp.producer.format != "normalized-ast-json" {
        return Err("structured C++ metadata must include producer version and `normalized-ast-json` format".into());
    }
    if cpp.compile.target_triple.trim().is_empty() {
        return Err("structured C++ metadata must include a target triple".into());
    }
    if !matches!(
        cpp.compile.language_standard.as_str(),
        "c++14" | "c++17" | "c++20" | "c++23"
    ) {
        return Err(format!(
            "unsupported C++ language standard `{}`",
            cpp.compile.language_standard
        ));
    }
    validate_compile_configuration(&cpp.compile, input_dir)?;
    validate_cpp_identifier_path("C++ namespace", &cpp.namespace)?;
    validate_input_path(input_dir, &cpp.header)?;
    let mut generated_header_names = BTreeSet::from([file_name(&cpp.header)?]);
    for header in &manifest.build.adapter_headers {
        let name = file_name(header)?;
        if !generated_header_names.insert(name.clone()) {
            return Err(format!(
                "duplicate generated C++ adapter header filename `{name}`"
            ));
        }
    }
    if cpp.sources.is_empty() {
        return Err("structured C++ metadata must declare at least one source".into());
    }
    for source in &cpp.sources {
        validate_input_path(input_dir, source)?;
    }

    let mut declarations = BTreeMap::new();
    for symbol in &cpp.symbols {
        if declarations.insert(symbol.id.as_str(), symbol).is_some() {
            return Err(format!(
                "duplicate structured C++ symbol id `{}`",
                symbol.id
            ));
        }
        validate_cpp_symbol(symbol)?;
    }
    if declarations.is_empty() {
        return Err("structured C++ metadata contains no symbols".into());
    }
    let policies = validate_mapping_policy(&manifest.mapping, &declarations)?;
    let symbols = ValidatedCppSymbols {
        declarations,
        policies,
    };
    validate_null_failure_policy(manifest, &symbols)?;
    if manifest.modules.is_empty() {
        return Err("native binding manifest must declare at least one module".into());
    }

    let mut operations = BTreeSet::new();
    for module in &manifest.modules {
        validate_identifier_path("module", &module.module)?;
        if module.documentation.trim().is_empty() {
            return Err(format!(
                "module `{}` documentation cannot be empty",
                module.module
            ));
        }
        for ty in &module.types {
            validate_upper_identifier("type", &ty.name)?;
            let symbol = symbols
                .declarations
                .get(ty.cpp_symbol.as_str())
                .ok_or_else(|| {
                    format!(
                        "type `{}` references unknown C++ symbol `{}`",
                        ty.name, ty.cpp_symbol
                    )
                })?;
            let expected_kind = match ty.kind {
                NativeBindingTypeKind::OpaqueResource | NativeBindingTypeKind::ValueRecord => {
                    CppSymbolKind::Record
                }
                NativeBindingTypeKind::Enum => CppSymbolKind::Enum,
            };
            if !symbols.is_bindable(&ty.cpp_symbol) || symbol.kind != expected_kind {
                return Err(format!(
                    "type `{}` must reference a bindable C++ {}",
                    ty.name,
                    match expected_kind {
                        CppSymbolKind::Record => "record",
                        CppSymbolKind::Enum => "enum",
                        _ => unreachable!("generated types only map records or enums"),
                    }
                ));
            }
            let policy = symbols
                .policies
                .get(ty.cpp_symbol.as_str())
                .expect("validated C++ symbol policy");
            match ty.kind {
                NativeBindingTypeKind::OpaqueResource => {
                    if !ty.variants.is_empty() {
                        return Err(format!(
                            "opaque resource `{}` cannot expose enum variants",
                            ty.name
                        ));
                    }
                    if !ty.fields.is_empty() {
                        return Err(format!(
                            "opaque resource `{}` cannot expose copied fields",
                            ty.name
                        ));
                    }
                    if policy.ownership != Some(CppOwnershipPolicy::Unique) {
                        return Err(stable_shape_error(
                            symbol,
                            UnsupportedCppShape::UnknownOwnership,
                        ));
                    }
                    if policy.thread_safety != Some(CppThreadSafetyPolicy::ThreadConfined) {
                        return Err(format!(
                            "type `{}` requires explicit package-owned thread-safety policy",
                            ty.name
                        ));
                    }
                }
                NativeBindingTypeKind::ValueRecord => {
                    if !ty.variants.is_empty() {
                        return Err(format!(
                            "value record `{}` cannot expose enum variants",
                            ty.name
                        ));
                    }
                    if policy.ownership != Some(CppOwnershipPolicy::Copied) {
                        return Err(format!(
                            "value record `{}` requires copied ownership policy",
                            ty.name
                        ));
                    }
                    validate_value_record(ty, symbol)?;
                }
                NativeBindingTypeKind::Enum => {
                    if !ty.fields.is_empty() {
                        return Err(format!(
                            "enum `{}` cannot expose copied record fields",
                            ty.name
                        ));
                    }
                    if policy.ownership.is_some() || policy.thread_safety.is_some() {
                        return Err(format!(
                            "enum `{}` cannot carry resource ownership policy",
                            ty.name
                        ));
                    }
                    validate_enum_mapping(ty, symbol)?;
                }
            }
        }
        for function in &module.functions {
            validate_lower_identifier("function", &function.name)?;
            validate_identifier_path("native operation", &function.operation)?;
            if !operations.insert(function.operation.as_str()) {
                return Err(format!(
                    "duplicate native operation `{}`",
                    function.operation
                ));
            }
            if function.documentation.trim().is_empty() {
                return Err(format!(
                    "function `{}` documentation cannot be empty",
                    function.name
                ));
            }
            for arg in &function.args {
                validate_lower_identifier("argument", &arg.name)?;
                reject_terlan_pointer_or_reference(&function.name, &arg.ty)?;
            }
            reject_terlan_pointer_or_reference(&function.name, &function.returns)?;
            if function.role != NativeFunctionRole::ExceptionMethod && function.fallible.is_some() {
                return Err(format!(
                    "function `{}` can declare fallible result types only for exception_method",
                    function.name
                ));
            }
            match function.role {
                NativeFunctionRole::Dispose => {
                    if function.cpp_symbol.is_some() {
                        return Err(format!("dispose function `{}` must be generated from handle ownership, not a C++ symbol", function.name));
                    }
                    if !function.projections.is_empty() {
                        return Err(format!(
                            "dispose function `{}` cannot declare value projections",
                            function.name
                        ));
                    }
                }
                NativeFunctionRole::ValueProjection => {
                    if function.cpp_symbol.is_some() {
                        return Err(format!(
                            "value projection `{}` must declare field symbols, not one C++ symbol",
                            function.name
                        ));
                    }
                    validate_value_projection(function, module, &symbols)?;
                }
                NativeFunctionRole::OwnedValueProjection => {
                    let id = function.cpp_symbol.as_deref().ok_or_else(|| {
                        format!(
                            "owned value projection `{}` requires cpp_symbol metadata",
                            function.name
                        )
                    })?;
                    let symbol = symbols.declarations.get(id).ok_or_else(|| {
                        format!(
                            "owned value projection `{}` references unknown C++ symbol `{id}`",
                            function.name
                        )
                    })?;
                    if !symbols.is_bindable(id) {
                        return Err(format!(
                            "owned value projection `{}` references rejected C++ symbol `{id}`",
                            function.name
                        ));
                    }
                    validate_owned_value_projection(function, symbol, &manifest.modules, &symbols)?;
                }
                NativeFunctionRole::EnumProjection => {
                    if !function.projections.is_empty() {
                        return Err(format!(
                            "enum projection `{}` cannot declare record projections",
                            function.name
                        ));
                    }
                    let id = function.cpp_symbol.as_deref().ok_or_else(|| {
                        format!(
                            "enum projection `{}` requires cpp_symbol metadata",
                            function.name
                        )
                    })?;
                    let symbol = symbols.declarations.get(id).ok_or_else(|| {
                        format!(
                            "enum projection `{}` references unknown C++ symbol `{id}`",
                            function.name
                        )
                    })?;
                    if !symbols.is_bindable(id) {
                        return Err(format!(
                            "enum projection `{}` references rejected C++ symbol `{id}`",
                            function.name
                        ));
                    }
                    validate_enum_projection(function, module, symbol, &symbols)?;
                }
                NativeFunctionRole::ExceptionMethod => {
                    if !function.projections.is_empty() {
                        return Err(format!(
                            "exception method `{}` cannot declare value projections",
                            function.name
                        ));
                    }
                    let id = function.cpp_symbol.as_deref().ok_or_else(|| {
                        format!(
                            "exception method `{}` requires cpp_symbol metadata",
                            function.name
                        )
                    })?;
                    let symbol = symbols.declarations.get(id).ok_or_else(|| {
                        format!(
                            "exception method `{}` references unknown C++ symbol `{id}`",
                            function.name
                        )
                    })?;
                    let policy = symbols
                        .policies
                        .get(id)
                        .expect("validated exception method policy");
                    validate_exception_method(function, module, symbol, policy, &symbols)?;
                }
                _ => {
                    if !function.projections.is_empty() {
                        return Err(format!(
                            "function `{}` cannot declare value projections for role `{}`",
                            function.name,
                            role_name(function.role)
                        ));
                    }
                    let id = function.cpp_symbol.as_deref().ok_or_else(|| {
                        format!("function `{}` requires cpp_symbol metadata", function.name)
                    })?;
                    let _symbol = symbols.declarations.get(id).ok_or_else(|| {
                        format!(
                            "function `{}` references unknown C++ symbol `{id}`",
                            function.name
                        )
                    })?;
                    if !symbols.is_bindable(id) {
                        return Err(format!(
                            "function `{}` references rejected C++ symbol `{id}`",
                            function.name
                        ));
                    }
                    validate_function_argument_mapping(
                        function,
                        _symbol,
                        &manifest.modules,
                        &symbols,
                    )?;
                    validate_function_return_mapping(function, _symbol, &manifest.modules)?;
                }
            }
        }
    }
    validate_resource_roles(manifest)?;
    Ok(symbols)
}

/// Validates a package-owned null-result classifier against extracted C++ facts.
fn validate_null_failure_policy(
    manifest: &NativeBindingManifest,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), String> {
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
        return Err(format!(
            "C++ null failure probe `{}` must be a bindable no-argument noexcept Int function",
            policy.probe_symbol
        ));
    }
    if manifest.modules.iter().any(|module| {
        module
            .functions
            .iter()
            .any(|function| function.cpp_symbol.as_deref() == Some(policy.probe_symbol.as_str()))
    }) {
        return Err(format!(
            "C++ null failure probe `{}` must remain hidden from Terlan modules",
            policy.probe_symbol
        ));
    }

    let mut values = BTreeSet::new();
    let mut codes = BTreeSet::new();
    for case in &policy.cases {
        if !values.insert(case.value) {
            return Err(format!(
                "duplicate C++ null failure status value `{}`",
                case.value
            ));
        }
        validate_stable_failure("C++ null failure case", &case.failure)?;
        if !codes.insert(case.failure.code.as_str()) {
            return Err(format!(
                "duplicate C++ null failure error code `{}`",
                case.failure.code
            ));
        }
    }
    validate_stable_failure("C++ null failure fallback", &policy.fallback)
}

/// Validates one finite package error without trusting native diagnostics.
fn validate_stable_failure(kind: &str, failure: &CppStableFailure) -> Result<(), String> {
    if failure.code.is_empty()
        || failure.code.len() > 128
        || !failure.code.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        })
    {
        return Err(format!("{kind} has an invalid package error code"));
    }
    if failure.message.trim().is_empty()
        || failure.message.len() > 4096
        || failure.message.chars().any(char::is_control)
    {
        return Err(format!("{kind} requires a stable single-line message"));
    }
    Ok(())
}

fn validate_cpp_symbol(symbol: &CppSymbol) -> Result<(), String> {
    if symbol.id.trim().is_empty() || symbol.cpp_name.trim().is_empty() {
        return Err("structured C++ symbols require stable id and cpp_name fields".into());
    }
    if symbol.source.path.trim().is_empty() || symbol.source.line == 0 || symbol.source.column == 0
    {
        return Err(format!(
            "structured C++ symbol `{}` requires a non-empty, one-based source location",
            symbol.id
        ));
    }
    if symbol.documentation.trim().is_empty() {
        return Err(format!(
            "structured C++ symbol `{}` requires extracted documentation",
            symbol.id
        ));
    }
    if symbol.overload_set.trim().is_empty() {
        return Err(format!(
            "structured C++ symbol `{}` requires stable overload-set identity",
            symbol.id
        ));
    }
    if symbol
        .annotations
        .iter()
        .any(|annotation| annotation.trim().is_empty())
    {
        return Err(format!(
            "structured C++ symbol `{}` contains an empty annotation",
            symbol.id
        ));
    }
    if let Some(returns) = &symbol.returns {
        validate_cpp_type_metadata(symbol, "return", returns)?;
    }
    for parameter in &symbol.parameters {
        validate_lower_identifier("C++ parameter", &parameter.name)?;
        validate_cpp_type_metadata(symbol, &parameter.name, &parameter.ty)?;
        if parameter.direction != CppParameterDirection::Input
            && parameter.ty.pointer_depth == 0
            && parameter.ty.reference == CppReferenceKind::None
        {
            return Err(format!(
                "C++ output parameter `{}::{}` requires pointer or reference type facts",
                symbol.id, parameter.name
            ));
        }
        if parameter
            .default
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(format!(
                "C++ parameter `{}::{}` has an empty default expression",
                symbol.id, parameter.name
            ));
        }
    }
    let mut field_names = BTreeSet::new();
    for field in &symbol.fields {
        validate_lower_identifier("C++ record field", &field.name)?;
        validate_cpp_type_metadata(symbol, &field.name, &field.ty)?;
        if !field_names.insert(field.name.as_str()) {
            return Err(format!(
                "structured C++ record `{}` contains duplicate field `{}`",
                symbol.id, field.name
            ));
        }
    }
    if symbol.kind != CppSymbolKind::Record && !symbol.fields.is_empty() {
        return Err(format!(
            "non-record C++ symbol `{}` cannot contain record fields",
            symbol.id
        ));
    }
    if symbol.kind != CppSymbolKind::Enum && !symbol.enum_values.is_empty() {
        return Err(format!(
            "non-enum C++ symbol `{}` cannot contain enumerators",
            symbol.id
        ));
    }
    if symbol.kind == CppSymbolKind::Enum {
        validate_cpp_enum(symbol)?;
    }
    if symbol.kind == CppSymbolKind::Method && symbol.receiver.is_none() {
        return Err(format!(
            "C++ method `{}` requires receiver metadata",
            symbol.id
        ));
    }
    Ok(())
}

/// Validates extractor-owned C++ enumerator names and discriminant spellings.
fn validate_cpp_enum(symbol: &CppSymbol) -> Result<(), String> {
    if symbol.enum_values.is_empty() {
        return Err(format!(
            "structured C++ enum `{}` requires at least one enumerator",
            symbol.id
        ));
    }
    let mut names = BTreeSet::new();
    for value in &symbol.enum_values {
        if !is_identifier_segment(&value.name) || !names.insert(value.name.as_str()) {
            return Err(format!(
                "structured C++ enum `{}` has invalid or duplicate enumerator `{}`",
                symbol.id, value.name
            ));
        }
        if value.value.is_empty()
            || !value
                .value
                .chars()
                .enumerate()
                .all(|(index, ch)| ch.is_ascii_digit() || (index == 0 && ch == '-'))
        {
            return Err(format!(
                "structured C++ enum `{}::{}` has invalid discriminant `{}`",
                symbol.id, value.name, value.value
            ));
        }
    }
    Ok(())
}

/// Validates a complete one-to-one copied field mapping.
fn validate_value_record(ty: &NativeBindingType, symbol: &CppSymbol) -> Result<(), String> {
    if ty.fields.is_empty() {
        return Err(format!(
            "value record `{}` requires at least one copied field",
            ty.name
        ));
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
            return Err(format!(
                "value record `{}` contains duplicate field `{}`",
                ty.name, field.name
            ));
        }
        let cpp_field = cpp_fields.get(field.cpp_field.as_str()).ok_or_else(|| {
            format!(
                "value record `{}.{}` references unknown C++ field `{}`",
                ty.name, field.name, field.cpp_field
            )
        })?;
        if !mapped_cpp_fields.insert(field.cpp_field.as_str()) {
            return Err(format!(
                "C++ field `{}` is mapped more than once by value record `{}`",
                field.cpp_field, ty.name
            ));
        }
        if !terlan_primitive_matches_cpp(&field.ty, &cpp_field.ty) {
            return Err(format!(
                "value record `{}.{}` maps incompatible Terlan type `{}` to C++ field `{}`",
                ty.name, field.name, field.ty, cpp_field.ty.canonical
            ));
        }
    }
    if mapped_cpp_fields.len() != cpp_fields.len() {
        return Err(format!(
            "value record `{}` must map all {} extracted C++ fields",
            ty.name,
            cpp_fields.len()
        ));
    }
    Ok(())
}

/// Validates a curated symbolic subset of one extractor-owned C++ enum.
fn validate_enum_mapping(ty: &NativeBindingType, symbol: &CppSymbol) -> Result<(), String> {
    if ty.variants.is_empty() {
        return Err(format!(
            "enum `{}` requires at least one reviewed symbolic variant",
            ty.name
        ));
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
            return Err(format!(
                "enum `{}.{}` references unknown C++ enumerator `{}`",
                ty.name, variant.name, variant.cpp_name
            ));
        }
        if !names.insert(variant.name.as_str())
            || !cpp_names.insert(variant.cpp_name.as_str())
            || !atoms.insert(variant.atom.as_str())
        {
            return Err(format!(
                "enum `{}` contains duplicate public names, C++ enumerators, or atoms",
                ty.name
            ));
        }
        validate_lower_identifier("enum atom", &variant.atom)?;
        if variant.documentation.trim().is_empty() {
            return Err(format!(
                "enum variant `{}.{}` documentation cannot be empty",
                ty.name, variant.name
            ));
        }
    }
    Ok(())
}

/// Validates one immutable resource method converted through a symbolic enum adapter.
fn validate_enum_projection(
    function: &NativeBindingFunction,
    module: &NativeBindingModule,
    symbol: &CppSymbol,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), String> {
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
            .is_some_and(|receiver| cpp_name_matches(receiver, &resource_symbol.cpp_name))
        || !symbol.parameters.is_empty()
        || !returns.is_some_and(|returns| {
            returns.enum_type && cpp_type_matches_symbol(&returns.canonical, enum_symbol)
        })
    {
        return Err(format!(
            "enum projection `{}` requires a bindable zero-argument const enum getter",
            function.name
        ));
    }
    Ok(())
}

/// Matches canonical qualified and declaration-local C++ type spellings.
fn cpp_type_matches_symbol(canonical: &str, symbol: &CppSymbol) -> bool {
    canonical == symbol.cpp_name
        || canonical == symbol.overload_set
        || canonical.ends_with(&format!("::{}", symbol.cpp_name))
}

/// Validates one throwing resource method and its public `Result` contract.
fn validate_exception_method(
    function: &NativeBindingFunction,
    module: &NativeBindingModule,
    symbol: &CppSymbol,
    policy: &CppSymbolPolicy,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), String> {
    let fallible = function.fallible.as_ref().ok_or_else(|| {
        format!(
            "exception method `{}` requires typed fallible result metadata",
            function.name
        )
    })?;
    if fallible.ok != "Int" || fallible.error != "std.core.Error.Error" {
        return Err(format!(
            "exception method `{}` currently requires Int success and std.core.Error.Error failure types",
            function.name
        ));
    }
    let expected_result = format!("Result[{}, {}]", fallible.ok, fallible.error);
    if function.returns != expected_result {
        return Err(format!(
            "exception method `{}` return type must be `{expected_result}`",
            function.name
        ));
    }
    if policy.exception.is_none() || symbol.noexcept {
        return Err(format!(
            "exception method `{}` requires a throwing C++ symbol with containment policy",
            function.name
        ));
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
        return Err(format!(
            "exception method `{}` currently requires a const resource method with Int arguments and result",
            function.name
        ));
    }
    Ok(())
}
