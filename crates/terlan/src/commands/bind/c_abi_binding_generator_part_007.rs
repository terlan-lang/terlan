
fn render_consumer_test(manifest: &CAbiBindingManifest) -> Result<String, String> {
    let module = &manifest.modules[0];
    if module
        .functions
        .iter()
        .all(|function| function.generated_smoke == CGeneratedSmokePolicy::PackageOwned)
    {
        return Ok(format!(
            "module {}.NativeBoundaryTest.\n\n@test\npub generated_c_abi_package_owns_live_validation(): Bool -> true.\n",
            manifest.package.namespace
        ));
    }
    let constructor = role_function(manifest, CAbiFunctionRole::Constructor)?;
    let reader = role_function(manifest, CAbiFunctionRole::ImmutableMethod)?;
    let dispose = dispose_for_type(manifest, &first_type(manifest)?.name)?;
    if constructor.args.len() != 1 || constructor.args[0].ty != "Int" || reader.returns != "Int" {
        return Err(
            "first generated consumer requires an Int constructor and Int reader".to_string(),
        );
    }
    let functions = &module.functions;
    let handle_type = first_type(manifest)?.name.as_str();
    let imports = functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let mut body = format!("    let boundary = {}(40);\n", constructor.name);
    let mut returned_handles = Vec::new();
    for function in functions {
        if function.generated_smoke == CGeneratedSmokePolicy::PackageOwned {
            continue;
        }
        if matches!(
            function.role,
            CAbiFunctionRole::Constructor | CAbiFunctionRole::Dispose
        ) || function
            .args
            .iter()
            .any(|argument| matches!(argument.ty.as_str(), "Float" | "Bool"))
            || (function.dispatcher.is_none()
                && function.returns == handle_type
                && function
                    .args
                    .iter()
                    .any(|argument| matches!(argument.ty.as_str(), "Int" | "List[Int]")))
        {
            continue;
        }
        let handle_argument_count = function
            .args
            .iter()
            .filter(|argument| argument.ty == handle_type)
            .count();
        let call_handle = if handle_argument_count > 1 {
            returned_handles
                .last()
                .map(|(_, variable, _): &(&str, String, bool)| variable.as_str())
                .ok_or_else(|| {
                    format!(
                        "generated consumer requires a preceding shaped handle for `{}`",
                        function.name
                    )
                })?
        } else {
            "boundary"
        };
        let args = function
            .args
            .iter()
            .map(|argument| {
                if argument.ty == handle_type {
                    Ok(call_handle)
                } else if argument.ty == "Int" {
                    Ok(if function.dispatcher.is_some() {
                        "-1"
                    } else {
                        "2"
                    })
                } else if argument.ty == "Float" {
                    Ok("2.5")
                } else if argument.ty == "Bool" {
                    Ok("true")
                } else if argument.ty == "List[Int]" {
                    Ok("[0]")
                } else {
                    Err(format!(
                        "first generated consumer cannot construct `{}`",
                        argument.ty
                    ))
                }
            })
            .collect::<Result<Vec<_>, String>>()?
            .join(", ");
        if function.returns == handle_type {
            let variable = format!("returned_{}", function.name);
            body.push_str(&format!(
                "    let {variable} = {}({args});\n",
                function.name
            ));
            returned_handles.push((
                function.name.as_str(),
                variable,
                handle_argument_count == 1 && function.args.len() == 1,
            ));
        } else if matches!(
            function.returns.as_str(),
            "Int" | "Float" | "Bool" | "List[Int]"
        ) {
            body.push_str(&format!(
                "    let observed_{} = {}({args});\n",
                function.name, function.name
            ));
        } else {
            body.push_str(&format!("    {}({args});\n", function.name));
        }
    }
    let mut assertions = vec![format!("observed_{} == 40", reader.name)];
    if returned_handles.iter().any(|(_, _, compare)| *compare) {
        body.push_str(&format!(
            "    let observed_returned_source_{} = {}(boundary);\n",
            reader.name, reader.name
        ));
    }
    for (function, variable, compare) in &returned_handles {
        body.push_str(&format!(
            "    let observed_{function}_{} = {}({variable});\n",
            reader.name, reader.name
        ));
        if *compare {
            assertions.push(format!(
                "observed_{function}_{} == observed_returned_source_{}",
                reader.name, reader.name
            ));
        }
    }
    for function in functions {
        let Some(symbol) = manifest
            .c_metadata
            .symbols
            .iter()
            .find(|symbol| symbol.id == function.c_symbol)
        else {
            continue;
        };
        for array in symbol
            .parameters
            .iter()
            .filter_map(|parameter| parameter.borrowed_array.as_ref())
        {
            let length_function = functions
                .iter()
                .find(|candidate| candidate.c_symbol == array.length_symbol)
                .ok_or_else(|| {
                    format!(
                        "generated consumer cannot map length symbol `{}`",
                        array.length_symbol
                    )
                })?;
            assertions.push(format!(
                "observed_{}.length() == observed_{}",
                function.name, length_function.name
            ));
        }
    }
    for (_, variable, _) in &returned_handles {
        body.push_str(&format!("    {}({variable});\n", dispose.name));
    }
    body.push_str(&format!(
        "    {}(boundary);\n    {}.\n",
        dispose.name,
        assertions.join(" and ")
    ));
    Ok(format!(
        "module {}.NativeBoundaryTest.\n\nimport {}.{{{imports}}}.\n\n@test\npub generated_c_abi_native_boundary_executes(): Bool ->\n{body}",
        manifest.package.namespace, module.module
    ))
}

fn render_skipped_symbols(
    metadata: &CMetadata,
    skipped: &[SkippedCSymbol],
) -> Result<String, String> {
    serde_json::to_string_pretty(&SkippedCSymbolsManifest {
        schema: SKIPPED_SYMBOLS_SCHEMA,
        metadata_producer: &metadata.producer,
        abi_version: metadata.abi_version,
        skipped,
    })
    .map(|text| text + "\n")
    .map_err(|error| format!("failed to render skipped C symbols manifest: {error}"))
}

fn first_type(manifest: &CAbiBindingManifest) -> Result<&CAbiBindingType, String> {
    manifest
        .modules
        .iter()
        .flat_map(|module| &module.types)
        .next()
        .ok_or_else(|| "first C ABI fixture requires one type".to_string())
}

fn binding_types(manifest: &CAbiBindingManifest) -> Vec<(&CAbiBindingModule, &CAbiBindingType)> {
    manifest
        .modules
        .iter()
        .flat_map(|module| module.types.iter().map(move |ty| (module, ty)))
        .collect()
}

fn binding_type<'a>(
    manifest: &'a CAbiBindingManifest,
    name: &str,
) -> Option<(&'a CAbiBindingModule, &'a CAbiBindingType)> {
    binding_types(manifest)
        .into_iter()
        .find(|(_, ty)| ty.name == name)
}

fn qualified_type_name(manifest: &CAbiBindingManifest, name: &str) -> Result<String, String> {
    binding_type(manifest, name)
        .map(|(module, ty)| format!("{}.{}", module.module, ty.name))
        .ok_or_else(|| format!("unknown C ABI opaque handle type `{name}`"))
}

fn dispose_for_type<'a>(
    manifest: &'a CAbiBindingManifest,
    type_name: &str,
) -> Result<&'a CAbiBindingFunction, String> {
    manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| {
            function.role == CAbiFunctionRole::Dispose
                && function.args.len() == 1
                && function.args[0].ty == type_name
        })
        .ok_or_else(|| format!("missing `dispose` C ABI fixture role for `{type_name}`"))
}

fn function_owner_type<'a>(
    manifest: &'a CAbiBindingManifest,
    function: &CAbiBindingFunction,
) -> Result<&'a CAbiBindingType, String> {
    let type_name = match function.role {
        CAbiFunctionRole::Constructor => function.returns.as_str(),
        CAbiFunctionRole::ImmutableMethod
        | CAbiFunctionRole::MutableMethod
        | CAbiFunctionRole::Dispose => function
            .args
            .first()
            .map(|argument| argument.ty.as_str())
            .ok_or_else(|| {
            format!(
                "error[native_bindgen.unsupported_wrapper_shape]: `{}` has no resource argument",
                function.name
            )
        })?,
        CAbiFunctionRole::FreeFunction => {
            return Err(format!(
                "error[native_bindgen.unsupported_wrapper_shape]: free function `{}` has no owner type",
                function.name
            ));
        }
    };
    binding_type(manifest, type_name)
        .map(|(_, ty)| ty)
        .ok_or_else(|| {
            format!(
                "error[native_bindgen.unsupported_terlan_type]: `{type_name}` in `{}` is not a declared opaque type",
                function.name
            )
        })
}

fn role_function(
    manifest: &CAbiBindingManifest,
    role: CAbiFunctionRole,
) -> Result<&CAbiBindingFunction, String> {
    manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.role == role)
        .ok_or_else(|| format!("missing `{}` C ABI fixture role", role_name(role)))
}

fn function_symbol<'a>(
    function: &CAbiBindingFunction,
    symbols: &BTreeMap<&str, &'a CSymbol>,
) -> Result<&'a CSymbol, String> {
    symbols
        .get(function.c_symbol.as_str())
        .copied()
        .ok_or_else(|| format!("unknown C symbol `{}`", function.c_symbol))
}

fn role_name(role: CAbiFunctionRole) -> &'static str {
    match role {
        CAbiFunctionRole::Constructor => "constructor",
        CAbiFunctionRole::ImmutableMethod => "immutable_method",
        CAbiFunctionRole::MutableMethod => "mutable_method",
        CAbiFunctionRole::FreeFunction => "free_function",
        CAbiFunctionRole::Dispose => "dispose",
    }
}

fn render_args(args: &[CAbiBindingArg]) -> String {
    args.iter()
        .map(|argument| format!("{}: {}", argument.name, argument.ty))
        .collect::<Vec<_>>()
        .join(", ")
}

fn blocking_policy_name(policy: &CAbiBlockingPolicy) -> &'static str {
    match policy {
        CAbiBlockingPolicy::Fast => "fast",
        CAbiBlockingPolicy::Blocking => "blocking",
        CAbiBlockingPolicy::Async => "async",
    }
}

fn resource_policy_name(policy: &CAbiResourcePolicy) -> &'static str {
    match policy {
        CAbiResourcePolicy::Value => "value",
        CAbiResourcePolicy::OpaqueHandle => "opaque_handle",
        CAbiResourcePolicy::BorrowedHandle => "borrowed_handle",
        CAbiResourcePolicy::MutableHandle => "mutable_handle",
        CAbiResourcePolicy::DisposeHandle => "dispose_handle",
        CAbiResourcePolicy::TransferableHandle => "transferable_handle",
    }
}

fn reject_terlan_pointer_or_reference(function: &str, ty: &str) -> Result<(), String> {
    if ty.contains('*') {
        return Err(format!(
            "error[native_bindgen.raw_pointer_ownership]: function `{function}` exposes `{ty}`"
        ));
    }
    if ty.contains('&') {
        return Err(format!(
            "error[native_bindgen.reference_lifetime_ambiguity]: function `{function}` exposes `{ty}`"
        ));
    }
    Ok(())
}

fn c_pointer_base(c_type: &str) -> &str {
    c_type
        .trim()
        .trim_start_matches("const ")
        .trim_end_matches(|character: char| character == '*' || character.is_ascii_whitespace())
        .trim()
}

fn resolve_c_type(c_type: &str, aliases: &BTreeMap<String, String>) -> Result<String, String> {
    let mut current = c_type.trim().to_string();
    let mut visited = BTreeSet::new();
    loop {
        let is_const = current.trim_start().starts_with("const ");
        let pointer_depth = current.matches('*').count();
        let base = c_pointer_base(&current).to_string();
        let Some(target) = aliases.get(&base) else {
            return Ok(current);
        };
        if !visited.insert(base.clone()) {
            return Err(format!("C alias cycle contains `{base}`"));
        }
        let mut resolved = target.trim().to_string();
        if is_const && !resolved.starts_with("const ") {
            resolved = format!("const {resolved}");
        }
        if pointer_depth > 0 {
            resolved.push(' ');
            resolved.push_str(&"*".repeat(pointer_depth));
        }
        current = resolved;
    }
}

fn validate_relative_metadata_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "external C metadata path `{value}` must be distribution-relative"
        ));
    }
    Ok(())
}

fn is_environment_variable(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_uppercase() || (index > 0 && byte.is_ascii_digit())
        })
}

fn is_link_library_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte == b'_' || byte == b'-' || byte.is_ascii_alphanumeric())
}

fn is_version_requirement(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte == b'.' || byte == b'-' || byte == b'+' || byte.is_ascii_alphanumeric()
        })
}

fn is_pinned_cargo_version(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn validate_input_path(input_dir: &Path, value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "C input path `{value}` must be a package-relative file"
        ));
    }
    if !input_dir.join(path).is_file() {
        return Err(format!(
            "structured C metadata input `{value}` does not exist"
        ));
    }
    Ok(())
}

fn file_name(value: &str) -> Result<String, String> {
    Path::new(value)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| format!("C input path `{value}` has no file name"))
}

fn validate_identifier_path(kind: &str, value: &str) -> Result<(), String> {
    if value.split('.').all(is_identifier_segment) {
        Ok(())
    } else {
        Err(format!("{kind} `{value}` must be a dotted identifier path"))
    }
}

fn validate_upper_identifier(kind: &str, value: &str) -> Result<(), String> {
    if is_identifier_segment(value) && value.chars().next().is_some_and(char::is_uppercase) {
        Ok(())
    } else {
        Err(format!(
            "{kind} `{value}` must start with an uppercase letter"
        ))
    }
}

fn validate_lower_identifier(kind: &str, value: &str) -> Result<(), String> {
    if is_identifier_segment(value) && value.chars().next().is_some_and(char::is_lowercase) {
        Ok(())
    } else {
        Err(format!(
            "{kind} `{value}` must start with a lowercase letter"
        ))
    }
}

fn validate_cargo_package_name(value: &str) -> Result<(), String> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err("C ABI binding package crate_name cannot be empty".into());
    };
    if !(first.is_ascii_lowercase() || first.is_ascii_digit())
        || !chars.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
        })
    {
        return Err(format!("invalid Cargo package name `{value}`"));
    }
    Ok(())
}

fn is_identifier_segment(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn is_c_identifier(value: &str) -> bool {
    is_identifier_segment(value)
}

fn module_source_path(module: &str) -> PathBuf {
    let mut path = PathBuf::from("src");
    for part in module.split('.') {
        path.push(part);
    }
    path.set_extension("terl");
    path
}

fn module_docs_path(module: &str) -> PathBuf {
    PathBuf::from("docs").join(format!("{module}.md"))
}

fn consumer_test_path(namespace: &str) -> PathBuf {
    let mut path = PathBuf::from("tests");
    for part in namespace.split('.') {
        path.push(part);
    }
    path.push("NativeBoundaryTest.terl");
    path
}

fn refuse_non_empty_output(out_dir: &Path) -> Result<(), String> {
    if !out_dir.exists() {
        return Ok(());
    }
    if fs::read_dir(out_dir)
        .map_err(|error| {
            format!(
                "failed to read output directory `{}`: {error}",
                out_dir.display()
            )
        })?
        .next()
        .transpose()
        .map_err(|error| {
            format!(
                "failed to inspect output directory `{}`: {error}",
                out_dir.display()
            )
        })?
        .is_some()
    {
        return Err(format!(
            "refusing to generate into non-empty output directory `{}`",
            out_dir.display()
        ));
    }
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("failed to create directory `{}`: {error}", parent.display())
        })?;
    }
    fs::copy(source, destination).map(|_| ()).map_err(|error| {
        format!(
            "failed to copy C input `{}` to `{}`: {error}",
            source.display(),
            destination.display()
        )
    })
}

fn write_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("failed to create directory `{}`: {error}", parent.display())
        })?;
    }
    fs::write(path, contents).map_err(|error| {
        format!(
            "failed to write generated file `{}`: {error}",
            path.display()
        )
    })
}
