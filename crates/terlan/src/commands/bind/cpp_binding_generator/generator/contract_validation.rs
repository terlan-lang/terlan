use super::*;

pub(super) fn validate_manifest<'a>(
    manifest: &'a NativeBindingManifest,
    input_dir: &Path,
) -> Result<ValidatedCppSymbols<'a>, CppBindingError> {
    if manifest.schema != NATIVE_BINDING_SCHEMA {
        return Err((format!(
            "unsupported native binding schema `{}`; expected `{NATIVE_BINDING_SCHEMA}`",
            manifest.schema
        ))
        .into());
    }
    if manifest.package.adapter != "cxx" {
        return Err((format!(
            "unsupported native binding adapter `{}`; expected `cxx`",
            manifest.package.adapter
        ))
        .into());
    }
    if manifest.mapping.schema != CPP_MAPPING_SCHEMA {
        return Err((format!(
            "unsupported C++ mapping schema `{}`; expected `{CPP_MAPPING_SCHEMA}`",
            manifest.mapping.schema
        ))
        .into());
    }
    validate_identifier_path("package namespace", &manifest.package.namespace)?;
    if manifest.package.namespace.starts_with("std.native") {
        return Err("generated external packages cannot use the `std.native` namespace".into());
    }
    validate_cargo_package_name(&manifest.package.crate_name)?;
    validate_build_plan(&manifest.build, input_dir)?;
    let cpp = &manifest.cpp_metadata;
    if cpp.schema != CPP_METADATA_SCHEMA {
        return Err((format!(
            "unsupported structured C++ metadata schema `{}`; expected `{CPP_METADATA_SCHEMA}`",
            cpp.schema
        ))
        .into());
    }
    if cpp.producer.name != "clang-libtooling" {
        return Err((format!(
            "unsupported C++ metadata producer `{}`; expected maintained tooling `clang-libtooling`",
            cpp.producer.name
        )).into());
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
        return Err((format!(
            "unsupported C++ language standard `{}`",
            cpp.compile.language_standard
        ))
        .into());
    }
    validate_compile_configuration(&cpp.compile, input_dir)?;
    validate_cpp_identifier_path("C++ namespace", &cpp.namespace)?;
    validate_input_path(input_dir, &cpp.header)?;
    let mut generated_header_names = BTreeSet::from([file_name(&cpp.header)?]);
    for header in &manifest.build.adapter_headers {
        let name = file_name(header)?;
        if !generated_header_names.insert(name.clone()) {
            return Err(
                (format!("duplicate generated C++ adapter header filename `{name}`")).into(),
            );
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
            return Err((format!("duplicate structured C++ symbol id `{}`", symbol.id)).into());
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
            return Err(
                (format!("module `{}` documentation cannot be empty", module.module)).into(),
            );
        }
        validate_type_imports(module, &manifest.modules)?;
        for ty in &module.types {
            validate_upper_identifier("type", &ty.name)?;
            if ty.kind != NativeBindingTypeKind::StringValue && ty.stringifier.is_some() {
                return Err((format!(
                    "type `{}` can declare a stringifier only for string_value",
                    ty.name
                ))
                .into());
            }
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
                NativeBindingTypeKind::OpaqueResource
                | NativeBindingTypeKind::ValueRecord
                | NativeBindingTypeKind::StringValue => CppSymbolKind::Record,
                NativeBindingTypeKind::Enum => CppSymbolKind::Enum,
            };
            if !symbols.is_bindable(&ty.cpp_symbol) || symbol.kind != expected_kind {
                return Err((format!(
                    "type `{}` must reference a bindable C++ {}",
                    ty.name,
                    match expected_kind {
                        CppSymbolKind::Record => "record",
                        CppSymbolKind::Enum => "enum",
                        _ => unreachable!("generated types only map records or enums"),
                    }
                ))
                .into());
            }
            let policy = symbols
                .policies
                .get(ty.cpp_symbol.as_str())
                .expect("validated C++ symbol policy");
            match ty.kind {
                NativeBindingTypeKind::OpaqueResource => {
                    if !ty.variants.is_empty() {
                        return Err((format!(
                            "opaque resource `{}` cannot expose enum variants",
                            ty.name
                        ))
                        .into());
                    }
                    if !ty.fields.is_empty() {
                        return Err((format!(
                            "opaque resource `{}` cannot expose copied fields",
                            ty.name
                        ))
                        .into());
                    }
                    if policy.ownership != Some(CppOwnershipPolicy::Unique) {
                        return Err((stable_shape_error(
                            symbol,
                            UnsupportedCppShape::UnknownOwnership,
                        ))
                        .into());
                    }
                    if policy.thread_safety != Some(CppThreadSafetyPolicy::ThreadConfined) {
                        return Err((format!(
                            "type `{}` requires explicit package-owned thread-safety policy",
                            ty.name
                        ))
                        .into());
                    }
                }
                NativeBindingTypeKind::ValueRecord => {
                    if !ty.variants.is_empty() {
                        return Err((format!(
                            "value record `{}` cannot expose enum variants",
                            ty.name
                        ))
                        .into());
                    }
                    if policy.ownership != Some(CppOwnershipPolicy::Copied) {
                        return Err((format!(
                            "value record `{}` requires copied ownership policy",
                            ty.name
                        ))
                        .into());
                    }
                    validate_value_record(ty, symbol)?;
                }
                NativeBindingTypeKind::Enum => {
                    if !ty.fields.is_empty() {
                        return Err((format!(
                            "enum `{}` cannot expose copied record fields",
                            ty.name
                        ))
                        .into());
                    }
                    if policy.ownership.is_some() || policy.thread_safety.is_some() {
                        return Err((format!(
                            "enum `{}` cannot carry resource ownership policy",
                            ty.name
                        ))
                        .into());
                    }
                    validate_enum_mapping(ty, symbol)?;
                }
                NativeBindingTypeKind::StringValue => {
                    if !ty.fields.is_empty() || !ty.variants.is_empty() {
                        return Err((format!(
                            "string value `{}` cannot expose fields or enum variants",
                            ty.name
                        ))
                        .into());
                    }
                    if policy.ownership != Some(CppOwnershipPolicy::Copied) {
                        return Err((format!(
                            "string value `{}` requires copied ownership policy",
                            ty.name
                        ))
                        .into());
                    }
                }
            }
        }
        for function in &module.functions {
            validate_lower_identifier("function", &function.name)?;
            validate_identifier_path("native operation", &function.operation)?;
            validate_argument_defaults(function)?;
            if !operations.insert(function.operation.as_str()) {
                return Err(
                    (format!("duplicate native operation `{}`", function.operation)).into(),
                );
            }
            if function.documentation.trim().is_empty() {
                return Err((format!(
                    "function `{}` documentation cannot be empty",
                    function.name
                ))
                .into());
            }
            for arg in &function.args {
                validate_lower_identifier("argument", &arg.name)?;
                reject_terlan_pointer_or_reference(&function.name, &arg.ty)?;
            }
            reject_terlan_pointer_or_reference(&function.name, &function.returns)?;
            if function.resource == NativeResourcePolicy::NullableHandle
                && manifest.null_failure.is_none()
            {
                return Err((format!(
                    "error[cpp.nullable_handle.failure_policy]: function `{}` requires a finite C++ null failure policy",
                    function.name
                )).into());
            }
            if function.role != NativeFunctionRole::ExceptionMethod && function.fallible.is_some() {
                return Err((format!(
                    "function `{}` can declare fallible result types only for exception_method",
                    function.name
                ))
                .into());
            }
            if let Some(body) = function.terlan_body.as_deref() {
                if function.cpp_symbol.is_some()
                    || !function.projections.is_empty()
                    || function.fallible.is_some()
                {
                    return Err((format!(
                        "error[cpp.terlan_body.native_shape]: Terlan-composed function `{}` cannot declare cpp_symbol, projections, or fallible native metadata",
                        function.name
                    )).into());
                }
                if body.trim().is_empty()
                    || body.chars().any(|character| character == '\0')
                    || body.trim_end().ends_with('.')
                {
                    return Err((format!(
                        "error[cpp.terlan_body.invalid]: function `{}` requires a non-empty, NUL-free Terlan expression without a trailing module terminator",
                        function.name
                    )).into());
                }
                continue;
            }
            match function.role {
                NativeFunctionRole::Dispose => {
                    if function.cpp_symbol.is_some() {
                        return Err((format!("dispose function `{}` must be generated from handle ownership, not a C++ symbol", function.name)).into());
                    }
                    if !function.projections.is_empty() {
                        return Err((format!(
                            "dispose function `{}` cannot declare value projections",
                            function.name
                        ))
                        .into());
                    }
                }
                NativeFunctionRole::ValueProjection => {
                    if function.cpp_symbol.is_some() {
                        return Err((format!(
                            "value projection `{}` must declare field symbols, not one C++ symbol",
                            function.name
                        ))
                        .into());
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
                        return Err((format!(
                            "owned value projection `{}` references rejected C++ symbol `{id}`",
                            function.name
                        ))
                        .into());
                    }
                    validate_owned_value_projection(function, symbol, &manifest.modules, &symbols)?;
                }
                NativeFunctionRole::EnumProjection => {
                    if !function.projections.is_empty() {
                        return Err((format!(
                            "enum projection `{}` cannot declare record projections",
                            function.name
                        ))
                        .into());
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
                        return Err((format!(
                            "enum projection `{}` references rejected C++ symbol `{id}`",
                            function.name
                        ))
                        .into());
                    }
                    validate_enum_projection(function, module, symbol, &symbols)?;
                }
                NativeFunctionRole::StringProjection => {
                    if !function.projections.is_empty() {
                        return Err((format!(
                            "string projection `{}` cannot declare record projections",
                            function.name
                        ))
                        .into());
                    }
                    let id = function.cpp_symbol.as_deref().ok_or_else(|| {
                        format!(
                            "string projection `{}` requires cpp_symbol metadata",
                            function.name
                        )
                    })?;
                    let symbol = symbols.declarations.get(id).ok_or_else(|| {
                        format!(
                            "string projection `{}` references unknown C++ symbol `{id}`",
                            function.name
                        )
                    })?;
                    if !symbols.is_bindable(id) {
                        return Err((format!(
                            "string projection `{}` references rejected C++ symbol `{id}`",
                            function.name
                        ))
                        .into());
                    }
                    validate_string_projection(function, module, symbol, &symbols)?;
                }
                NativeFunctionRole::ScalarProjection => {
                    if !function.projections.is_empty() {
                        return Err((format!(
                            "integer projection `{}` cannot declare record projections",
                            function.name
                        ))
                        .into());
                    }
                    let id = function.cpp_symbol.as_deref().ok_or_else(|| {
                        format!(
                            "integer projection `{}` requires cpp_symbol metadata",
                            function.name
                        )
                    })?;
                    let symbol = symbols.declarations.get(id).ok_or_else(|| {
                        format!(
                            "integer projection `{}` references unknown C++ symbol `{id}`",
                            function.name
                        )
                    })?;
                    let policy = symbols
                        .policies
                        .get(id)
                        .expect("validated integer projection policy");
                    validate_scalar_projection(
                        function,
                        symbol,
                        policy,
                        &manifest.modules,
                        &symbols,
                    )?;
                }
                NativeFunctionRole::IntListProjection => {
                    if !function.projections.is_empty() {
                        return Err((format!(
                            "integer-list projection `{}` cannot declare record projections",
                            function.name
                        ))
                        .into());
                    }
                    let id = function.cpp_symbol.as_deref().ok_or_else(|| {
                        format!(
                            "integer-list projection `{}` requires cpp_symbol metadata",
                            function.name
                        )
                    })?;
                    let symbol = symbols.declarations.get(id).ok_or_else(|| {
                        format!(
                            "integer-list projection `{}` references unknown C++ symbol `{id}`",
                            function.name
                        )
                    })?;
                    if !symbols.is_bindable(id) {
                        return Err((format!(
                            "integer-list projection `{}` references rejected C++ symbol `{id}`",
                            function.name
                        ))
                        .into());
                    }
                    validate_int_list_projection(function, module, symbol, &symbols)?;
                }
                NativeFunctionRole::ResourceListProjection => {
                    if !function.projections.is_empty() {
                        return Err((format!(
                            "resource-list projection `{}` cannot declare record projections",
                            function.name
                        ))
                        .into());
                    }
                    let id = function.cpp_symbol.as_deref().ok_or_else(|| {
                        format!(
                            "resource-list projection `{}` requires cpp_symbol metadata",
                            function.name
                        )
                    })?;
                    let symbol = symbols.declarations.get(id).ok_or_else(|| {
                        format!(
                            "resource-list projection `{}` references unknown C++ symbol `{id}`",
                            function.name
                        )
                    })?;
                    let policy = symbols
                        .policies
                        .get(id)
                        .expect("validated resource-list policy");
                    validate_resource_list_projection(
                        manifest, function, module, symbol, policy, &symbols,
                    )?;
                }
                NativeFunctionRole::MutableFreeFunction => {
                    if !function.projections.is_empty() {
                        return Err((format!(
                            "mutable free function `{}` cannot declare value projections",
                            function.name
                        ))
                        .into());
                    }
                    let id = function.cpp_symbol.as_deref().ok_or_else(|| {
                        format!(
                            "mutable free function `{}` requires cpp_symbol metadata",
                            function.name
                        )
                    })?;
                    let symbol = symbols.declarations.get(id).ok_or_else(|| {
                        format!(
                            "mutable free function `{}` references unknown C++ symbol `{id}`",
                            function.name
                        )
                    })?;
                    if !symbols.is_bindable(id) {
                        return Err((format!(
                            "mutable free function `{}` references rejected C++ symbol `{id}`",
                            function.name
                        ))
                        .into());
                    }
                    let policy = symbols
                        .policies
                        .get(id)
                        .expect("validated mutable free-function policy");
                    validate_mutable_free_function(
                        function,
                        symbol,
                        policy,
                        &manifest.modules,
                        &symbols,
                    )?;
                }
                NativeFunctionRole::ExceptionMethod => {
                    if !function.projections.is_empty() {
                        return Err((format!(
                            "exception method `{}` cannot declare value projections",
                            function.name
                        ))
                        .into());
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
                        return Err((format!(
                            "function `{}` cannot declare value projections for role `{}`",
                            function.name,
                            role_name(function.role)
                        ))
                        .into());
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
                        return Err((format!(
                            "function `{}` references rejected C++ symbol `{id}`",
                            function.name
                        ))
                        .into());
                    }
                    validate_function_argument_mapping(
                        function,
                        _symbol,
                        &manifest.modules,
                        &symbols,
                    )?;
                    validate_function_return_mapping(
                        function,
                        _symbol,
                        &manifest.modules,
                        &symbols,
                    )?;
                }
            }
        }
    }
    validate_resource_roles(manifest)?;
    for symbol in symbols.declarations.values().filter(|symbol| {
        symbols.is_bindable(&symbol.id)
            && symbol.kind == CppSymbolKind::Record
            && !symbol.inheritance.is_empty()
    }) {
        let opaque = manifest
            .modules
            .iter()
            .flat_map(|module| &module.types)
            .any(|ty| {
                ty.kind == NativeBindingTypeKind::OpaqueResource && ty.cpp_symbol == symbol.id
            });
        if !opaque {
            return Err(
                (stable_shape_error(symbol, UnsupportedCppShape::UnsupportedInheritance)).into(),
            );
        }
    }
    for symbol in symbols.declarations.values().filter(|symbol| {
        symbols.is_bindable(&symbol.id)
            && !matches!(symbol.kind, CppSymbolKind::Record | CppSymbolKind::Enum)
    }) {
        let has_unmapped_shape = symbol.returns.as_ref().is_some_and(|returns| {
            returns.canonical != "void" && rust_bridge_type(returns).is_err()
        }) || symbol
            .parameters
            .iter()
            .any(|parameter| rust_bridge_type(&parameter.ty).is_err());
        let generated_adapter = manifest.modules.iter().any(|module| {
            module.functions.iter().any(|function| {
                function.cpp_symbol.as_deref() == Some(symbol.id.as_str())
                    && (matches!(
                        function.role,
                        NativeFunctionRole::EnumProjection
                            | NativeFunctionRole::StringProjection
                            | NativeFunctionRole::ScalarProjection
                            | NativeFunctionRole::IntListProjection
                            | NativeFunctionRole::ResourceListProjection
                            | NativeFunctionRole::ExceptionMethod
                    ) || function_uses_mutation_adapter(manifest, function)
                        || function_owned_value_resource(manifest, function, &symbols.declarations)
                            .is_some()
                        || function_owned_value_resource_tuple(
                            manifest,
                            function,
                            &symbols.declarations,
                        )
                        .is_some())
            })
        }) || manifest.modules.iter().any(|module| {
            module.functions.iter().any(|function| {
                if function.role != NativeFunctionRole::StringProjection {
                    return false;
                }
                module.types.iter().any(|ty| {
                    ty.kind == NativeBindingTypeKind::StringValue
                        && terlan_type_matches(&function.returns, &ty.name)
                        && ty.stringifier.as_deref() == Some(symbol.id.as_str())
                })
            })
        });
        if symbol.overload_candidates > 1 && !generated_adapter {
            return Err(
                (stable_shape_error(symbol, UnsupportedCppShape::OverloadAmbiguity)).into(),
            );
        }
        if has_unmapped_shape && !generated_adapter {
            return Err((stable_shape_error(symbol, UnsupportedCppShape::UnmappedType)).into());
        }
    }
    Ok(symbols)
}

/// Requires module imports to resolve to one generated sibling-owned type.
fn validate_type_imports(
    module: &NativeBindingModule,
    modules: &[NativeBindingModule],
) -> Result<(), CppBindingError> {
    let mut imports = BTreeSet::new();
    for imported in &module.type_imports {
        validate_identifier_path("generated type import", imported)?;
        if !imports.insert(imported.as_str()) {
            return Err((format!("duplicate generated type import `{imported}`")).into());
        }
        let Some((owner, type_name)) = imported.rsplit_once('.') else {
            return Err((format!(
                "generated type import `{imported}` must include its owning module"
            ))
            .into());
        };
        if owner == module.module {
            return Err((format!(
                "module `{}` cannot import its own generated type `{type_name}`",
                module.module
            ))
            .into());
        }
        let resolves = modules.iter().any(|candidate| {
            candidate.module == owner && candidate.types.iter().any(|ty| ty.name == type_name)
        });
        if !resolves {
            return Err((format!(
                "generated type import `{imported}` does not resolve to a sibling module type"
            ))
            .into());
        }
    }
    Ok(())
}

/// Validates public defaults as Terlan literals and trailing arguments.
fn validate_argument_defaults(function: &NativeBindingFunction) -> Result<(), CppBindingError> {
    let mut saw_default = false;
    for argument in &function.args {
        let Some(default) = &argument.default else {
            if saw_default {
                return Err((format!(
                    "function `{}` has required argument `{}` after a defaulted argument",
                    function.name, argument.name
                ))
                .into());
            }
            continue;
        };
        saw_default = true;
        let valid = match argument.ty.as_str() {
            "Int" => default.parse::<i64>().is_ok(),
            "Float" => default.parse::<f64>().is_ok_and(|value| value.is_finite()),
            "Bool" => matches!(default.as_str(), "true" | "false"),
            "String" => serde_json::from_str::<String>(default).is_ok(),
            "List[Int]" => serde_json::from_str::<Vec<i64>>(default).is_ok(),
            _ => false,
        };
        if !valid {
            return Err((format!(
                "function `{}` argument `{}` has invalid default `{default}` for `{}`",
                function.name, argument.name, argument.ty
            ))
            .into());
        }
    }
    Ok(())
}
