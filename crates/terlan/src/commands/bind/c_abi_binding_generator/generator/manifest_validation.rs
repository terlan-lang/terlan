//! Validation of C binding manifests against extracted symbols.

use super::*;

pub(super) fn validate_manifest<'a>(
    manifest: &'a CAbiBindingManifest,
    input_dir: &Path,
) -> Result<BTreeMap<&'a str, &'a CSymbol>, String> {
    if manifest.schema != C_ABI_BINDING_SCHEMA {
        return Err(format!(
            "unsupported C ABI binding schema `{}`; expected `{C_ABI_BINDING_SCHEMA}`",
            manifest.schema
        ));
    }
    if manifest.package.adapter != "c-abi" {
        return Err(format!(
            "unsupported C ABI binding adapter `{}`; expected `c-abi`",
            manifest.package.adapter
        ));
    }
    validate_identifier_path("package namespace", &manifest.package.namespace)?;
    if manifest.package.namespace.starts_with("std.native") {
        return Err("generated external packages cannot use the `std.native` namespace".into());
    }
    validate_cargo_package_name(&manifest.package.crate_name)?;
    if let Some(name) = &manifest.package.name {
        validate_cargo_package_name(name)?;
    }
    if let Some(version) = &manifest.package.version {
        if !is_pinned_cargo_version(version) {
            return Err(format!(
                "C ABI binding package version must use exact x.y.z form; found `{version}`"
            ));
        }
    }
    validate_terlan_dependencies(&manifest.package)?;
    validate_rust_extension(&manifest.package, input_dir)?;
    validate_c_abi_contract(&manifest.validation)?;

    let metadata = &manifest.c_metadata;
    if metadata.schema != C_METADATA_SCHEMA {
        return Err(format!(
            "unsupported structured C metadata schema `{}`; expected `{C_METADATA_SCHEMA}`",
            metadata.schema
        ));
    }
    let producer_format = match metadata.producer.name.as_str() {
        "clang-libtooling" => "normalized-ast-json",
        "terlan-curated-metadata" => "reviewed-declaration-json",
        producer => {
            return Err(format!(
                "unsupported C metadata producer `{producer}`; expected maintained tooling `clang-libtooling` or `terlan-curated-metadata`"
            ));
        }
    };
    if metadata.producer.version.trim().is_empty() || metadata.producer.format != producer_format {
        return Err(format!(
            "structured C metadata from `{}` requires a producer version and `{producer_format}` format",
            metadata.producer.name
        ));
    }
    if metadata.abi_version == 0 {
        return Err("error[native_bindgen.c_abi_version_missing]: C metadata requires a positive ABI version".into());
    }
    validate_c_inputs(metadata, input_dir)?;
    if let Some(standard) = &metadata.cpp_standard {
        if !matches!(standard.as_str(), "c++17" | "c++20") {
            return Err(format!(
                "C ABI package C++ standard must be `c++17` or `c++20`; found `{standard}`"
            ));
        }
        if !metadata
            .sources
            .iter()
            .any(|source| is_cpp_adapter_source(source))
        {
            return Err("C ABI package C++ standard requires a declared C++ source".into());
        }
    }
    validate_c_aliases(metadata)?;

    let mut symbols = BTreeMap::new();
    for symbol in &metadata.symbols {
        if symbols.insert(symbol.id.as_str(), symbol).is_some() {
            return Err(format!("duplicate structured C symbol id `{}`", symbol.id));
        }
        validate_c_symbol(symbol, &metadata.aliases)?;
    }
    if symbols.is_empty() {
        return Err("structured C metadata contains no symbols".into());
    }
    for symbol in symbols.values().filter(|symbol| {
        symbol.status == CSymbolStatus::Bind && symbol.kind == CSymbolKind::OpaqueStruct
    }) {
        let destructor_id = symbol
            .destructor_symbol
            .as_deref()
            .ok_or_else(|| stable_shape_error(symbol, UnsupportedCShape::MissingDestructor))?;
        let destructor = symbols.get(destructor_id).ok_or_else(|| {
            format!(
                "opaque C symbol `{}` references unknown destructor `{destructor_id}`",
                symbol.id
            )
        })?;
        if destructor.status != CSymbolStatus::Bind || destructor.kind != CSymbolKind::Function {
            return Err(format!(
                "opaque C symbol `{}` requires a bindable destructor function",
                symbol.id
            ));
        }
    }
    validate_c_type_references(&symbols, &metadata.aliases)?;
    validate_borrowed_arrays(&symbols, &metadata.aliases)?;
    validate_owned_strings(&symbols, &metadata.aliases)?;
    validate_owned_arrays(&symbols, &metadata.aliases)?;
    validate_owned_string_arrays(&symbols, &metadata.aliases)?;
    validate_owned_handle_arrays(&symbols, &metadata.aliases)?;

    if manifest.modules.is_empty() {
        return Err("C ABI binding manifest must declare at least one module".into());
    }
    validate_terlan_module_extensions(manifest, input_dir)?;
    let mut operations = BTreeSet::new();
    for module in &manifest.modules {
        validate_identifier_path("module", &module.module)?;
        let mut imported_names = BTreeSet::new();
        for import in &module.imports {
            validate_identifier_path("import module", &import.module)?;
            if import.names.is_empty() {
                return Err(format!(
                    "module `{}` import `{}` must name at least one declaration",
                    module.module, import.module
                ));
            }
            for name in &import.names {
                if name.chars().next().is_some_and(char::is_uppercase) {
                    validate_upper_identifier("import", name)?;
                } else {
                    validate_lower_identifier("import", name)?;
                }
                if !imported_names.insert((import.module.as_str(), name.as_str())) {
                    return Err(format!(
                        "module `{}` repeats import `{}.{name}`",
                        module.module, import.module
                    ));
                }
            }
        }
        if module.documentation.trim().is_empty() {
            return Err(format!(
                "module `{}` documentation cannot be empty",
                module.module
            ));
        }
        for ty in &module.types {
            validate_upper_identifier("type", &ty.name)?;
            let symbol = symbols.get(ty.c_symbol.as_str()).ok_or_else(|| {
                format!(
                    "type `{}` references unknown C symbol `{}`",
                    ty.name, ty.c_symbol
                )
            })?;
            if symbol.status != CSymbolStatus::Bind || symbol.kind != CSymbolKind::OpaqueStruct {
                return Err(format!(
                    "type `{}` must reference a bindable opaque C struct",
                    ty.name
                ));
            }
        }
        for function in &module.functions {
            validate_lower_identifier("function", &function.name)?;
            validate_lower_identifier("adapter function", function.adapter_name())?;
            validate_identifier_path("native operation", &function.operation)?;
            validate_argument_defaults(function)?;
            validate_argument_mutability(manifest, function)?;
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
            for argument in &function.args {
                validate_lower_identifier("argument", &argument.name)?;
                reject_terlan_pointer_or_reference(&function.name, &argument.ty)?;
                reject_terlan_pointer_or_reference(&function.name, argument.abi_ty())?;
            }
            reject_terlan_pointer_or_reference(&function.name, &function.returns)?;
            let symbol = symbols.get(function.c_symbol.as_str()).ok_or_else(|| {
                format!(
                    "function `{}` references unknown C symbol `{}`",
                    function.name, function.c_symbol
                )
            })?;
            if symbol.status != CSymbolStatus::Bind || symbol.kind != CSymbolKind::Function {
                return Err(format!(
                    "function `{}` requires a bindable C function",
                    function.name
                ));
            }
            if let Some(dispatcher) = &function.dispatcher {
                validate_dispatcher_binding(
                    function,
                    dispatcher,
                    symbol,
                    &symbols,
                    &metadata.aliases,
                )?;
            } else {
                validate_input_array_binding(manifest, function, symbol, &metadata.aliases)?;
                validate_owned_string_binding(function, symbol)?;
                validate_owned_array_binding(function, symbol)?;
                validate_owned_string_array_binding(function, symbol)?;
                validate_owned_handle_array_binding(manifest, function, symbol)?;
            }
        }
    }
    validate_binding_roles(manifest)?;
    for (_, bound_type) in binding_types(manifest) {
        let record = symbols
            .get(bound_type.c_symbol.as_str())
            .ok_or_else(|| format!("unknown C record `{}`", bound_type.c_symbol))?;
        let dispose = dispose_for_type(manifest, &bound_type.name)?;
        if record.destructor_symbol.as_deref() != Some(dispose.c_symbol.as_str()) {
            return Err(format!(
                "dispose function `{}` for `{}` must reference destructor `{}`",
                dispose.name,
                bound_type.name,
                record.destructor_symbol.as_deref().unwrap_or("")
            ));
        }
    }
    Ok(symbols)
}

pub(super) fn validate_c_abi_contract(contract: &CAbiValidationContract) -> Result<(), String> {
    for (name, enabled) in [
        (
            "deterministic_regeneration",
            contract.deterministic_regeneration,
        ),
        ("warning_denied_build", contract.warning_denied_build),
        ("ownership_lifecycle", contract.ownership_lifecycle),
        ("error_translation", contract.error_translation),
    ] {
        if !enabled {
            return Err(format!(
                "error[native_bindgen.c_abi_validation]: C ABI validation obligation `{name}` must be enabled"
            ));
        }
    }
    match contract.smoke {
        CAbiSmokeValidation::GeneratedFixture | CAbiSmokeValidation::PackageOwnedLive => Ok(()),
    }
}
