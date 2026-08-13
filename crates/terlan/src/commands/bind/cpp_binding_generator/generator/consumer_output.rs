use super::*;

/// Renders an executable lifecycle test when a module exposes every operation
/// needed to establish and verify resource ownership.
///
/// Modules made only of static functions, copied types, or partial resource
/// APIs return `None`; generating an unconditional passing test for those
/// modules would falsely imply runtime coverage.
pub(super) fn render_consumer_test(module: &NativeBindingModule) -> Result<Option<String>, String> {
    let test_module = format!("{}Test", module.module);
    let Some(constructor) = optional_role_function(module, NativeFunctionRole::Constructor) else {
        return Ok(None);
    };
    let Some(reader) = optional_role_function(module, NativeFunctionRole::ImmutableMethod) else {
        return Ok(None);
    };
    let Some(mutator) = optional_role_function(module, NativeFunctionRole::MutableMethod) else {
        return Ok(None);
    };
    let Some(dispose) = optional_role_function(module, NativeFunctionRole::Dispose) else {
        return Ok(None);
    };
    let Some(counter) = module.functions.iter().find(|function| {
        function.role == NativeFunctionRole::FreeFunction && function.args.is_empty()
    }) else {
        return Ok(None);
    };
    let projection = module
        .functions
        .iter()
        .find(|function| function.role == NativeFunctionRole::ValueProjection);
    let mut imports = vec![
        constructor.name.as_str(),
        reader.name.as_str(),
        mutator.name.as_str(),
        dispose.name.as_str(),
        counter.name.as_str(),
    ];
    let copied_functions = module
        .functions
        .iter()
        .filter(|function| {
            function.role == NativeFunctionRole::ImmutableMethod
                && matches!(
                    function.returns.as_str(),
                    "String" | "std.vm.Bytes.Bytes" | "List[Int]" | "List[Float]"
                )
        })
        .collect::<Vec<_>>();
    imports.extend(
        copied_functions
            .iter()
            .map(|function| function.name.as_str()),
    );
    let copied_steps = copied_functions
        .iter()
        .map(|function| {
            format!(
                "    let copied_{} = {}(boundary);\n",
                function.name, function.name
            )
        })
        .collect::<String>();
    let mut enum_steps = String::new();
    let mut enum_checks = String::new();
    for function in module
        .functions
        .iter()
        .filter(|function| function.role == NativeFunctionRole::EnumProjection)
    {
        let enum_type = module
            .types
            .iter()
            .find(|ty| {
                ty.kind == NativeBindingTypeKind::Enum
                    && terlan_type_matches(&function.returns, &ty.name)
            })
            .ok_or_else(|| format!("enum projection `{}` has no result type", function.name))?;
        imports.push(function.name.as_str());
        imports.extend(
            enum_type
                .variants
                .iter()
                .map(|variant| variant.name.as_str()),
        );
        enum_steps.push_str(&format!(
            "    let copied_{} = {}(boundary);\n",
            function.name, function.name
        ));
        enum_checks.push_str(&format!(
            " and ({})",
            enum_type
                .variants
                .iter()
                .map(|variant| format!("copied_{} == {}", function.name, variant.name))
                .collect::<Vec<_>>()
                .join(" or ")
        ));
    }
    let mut exception_steps = String::new();
    let exception_checks = String::new();
    for function in module
        .functions
        .iter()
        .filter(|function| function.role == NativeFunctionRole::ExceptionMethod)
    {
        imports.push(function.name.as_str());
        exception_steps.push_str(&format!(
            "    let _contained_{} = {}(boundary);\n",
            function.name, function.name
        ));
    }
    let (projection_step, projection_check) = if let Some(projection) = projection {
        imports.push(projection.name.as_str());
        let record = module
            .types
            .iter()
            .find(|ty| {
                ty.kind == NativeBindingTypeKind::ValueRecord
                    && terlan_type_matches(&projection.returns, &ty.name)
            })
            .ok_or_else(|| format!("projection `{}` has no value record", projection.name))?;
        imports.push(record.name.as_str());
        let reader_symbol = reader
            .cpp_symbol
            .as_deref()
            .ok_or_else(|| format!("reader `{}` has no C++ symbol", reader.name))?;
        let field = projection
            .projections
            .iter()
            .find(|field| field.cpp_symbol == reader_symbol)
            .ok_or_else(|| {
                format!(
                    "projection `{}` must include the reader value for its consumer test",
                    projection.name
                )
            })?;
        (
            format!("    let copied = {}(boundary);\n", projection.name),
            format!(" and copied.{} == observed", field.field),
        )
    } else {
        (String::new(), String::new())
    };
    let (record_input_step, record_input_check) = projection
        .and_then(|projection| {
            module.functions.iter().find(|function| {
                function.role == NativeFunctionRole::FreeFunction
                    && function.args.len() == 1
                    && !function.args[0].fields.is_empty()
                    && terlan_type_matches(&function.args[0].ty, &projection.returns)
            })
        })
        .map(|function| {
            imports.push(function.name.as_str());
            (
                format!(
                    "    let copied_record_result = {}(copied);\n",
                    function.name
                ),
                " and copied_record_result == copied_record_result".to_string(),
            )
        })
        .unwrap_or_default();
    imports.sort_unstable();
    imports.dedup();
    Ok(Some(format!(
        "module {}.\n\nimport {}.{{{}}}.\n\n@test\npub generated_cpp_resource_executes(): Bool ->\n    let boundary = {}(40);\n    {}(boundary, 2);\n    let observed = {}(boundary);\n{}{}{}{}{}    {}(boundary);\n    observed == 42{}{}{}{} and {}() == 0.\n",
        test_module,
        module.module,
        imports.join(", "),
        constructor.name,
        mutator.name,
        reader.name,
        projection_step,
        record_input_step,
        copied_steps,
        enum_steps,
        exception_steps,
        dispose.name,
        projection_check,
        record_input_check,
        enum_checks,
        exception_checks,
        counter.name
    )))
}

pub(super) fn render_skipped_symbols(
    producer: &CppMetadataProducer,
    skipped: &[SkippedSymbol],
) -> Result<String, String> {
    serde_json::to_string_pretty(&SkippedSymbolsManifest {
        schema: SKIPPED_SYMBOLS_SCHEMA,
        metadata_producer: producer,
        skipped,
    })
    .map(|text| text + "\n")
    .map_err(|err| format!("failed to render skipped native symbols manifest: {err}"))
}

/// Returns the first function with `role` without requiring fixture-style roles.
pub(super) fn optional_role_function(
    module: &NativeBindingModule,
    role: NativeFunctionRole,
) -> Option<&NativeBindingFunction> {
    module
        .functions
        .iter()
        .find(|function| function.role == role)
}

pub(super) fn function_symbol<'a>(
    function: &NativeBindingFunction,
    symbols: &BTreeMap<&str, &'a CppSymbol>,
) -> Result<&'a CppSymbol, String> {
    let id = function
        .cpp_symbol
        .as_deref()
        .ok_or_else(|| format!("function `{}` has no C++ symbol", function.name))?;
    symbols
        .get(id)
        .copied()
        .ok_or_else(|| format!("unknown C++ symbol `{id}`"))
}

pub(super) fn role_name(role: NativeFunctionRole) -> &'static str {
    match role {
        NativeFunctionRole::Constructor => "constructor",
        NativeFunctionRole::ImmutableMethod => "immutable_method",
        NativeFunctionRole::MutableMethod => "mutable_method",
        NativeFunctionRole::FreeFunction => "free_function",
        NativeFunctionRole::ValueProjection => "value_projection",
        NativeFunctionRole::OwnedValueProjection => "owned_value_projection",
        NativeFunctionRole::EnumProjection => "enum_projection",
        NativeFunctionRole::ExceptionMethod => "exception_method",
        NativeFunctionRole::Dispose => "dispose",
    }
}

pub(super) fn render_args(args: &[NativeBindingArg]) -> String {
    args.iter()
        .map(|arg| format!("{}: {}", arg.name, arg.ty))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Converts one generated PascalCase type name into a public constructor name.
pub(super) fn lower_type_name(value: &str) -> String {
    let mut result = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                result.push('_');
            }
            result.push(character.to_ascii_lowercase());
        } else {
            result.push(character);
        }
    }
    result
}

pub(super) fn blocking_policy_name(policy: &NativeBlockingPolicy) -> &'static str {
    match policy {
        NativeBlockingPolicy::Fast => "fast",
        NativeBlockingPolicy::Blocking => "blocking",
        NativeBlockingPolicy::Async => "async",
    }
}

pub(super) fn resource_policy_name(policy: &NativeResourcePolicy) -> &'static str {
    match policy {
        NativeResourcePolicy::Value => "value",
        NativeResourcePolicy::OpaqueHandle => "opaque_handle",
        NativeResourcePolicy::OwnedHandle => "owned_handle",
        NativeResourcePolicy::NullableHandle => "nullable_handle",
        NativeResourcePolicy::BorrowedHandle => "borrowed_handle",
        NativeResourcePolicy::MutableHandle => "mutable_handle",
        NativeResourcePolicy::DisposeHandle => "dispose_handle",
        NativeResourcePolicy::TransferableHandle => "transferable_handle",
    }
}

pub(super) fn reject_terlan_pointer_or_reference(function: &str, ty: &str) -> Result<(), String> {
    if ty.contains('*') {
        return Err(format!(
            "error[cpp.pointer.unsupported]: function `{function}` exposes `{ty}`"
        ));
    }
    if ty.contains('&') {
        return Err(format!(
            "error[cpp.lifetime.borrowed]: function `{function}` exposes `{ty}`"
        ));
    }
    Ok(())
}

pub(super) fn validate_input_path(input_dir: &Path, value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "C++ input path `{value}` must be a package-relative file"
        ));
    }
    if !input_dir.join(path).is_file() {
        return Err(format!(
            "structured C++ metadata input `{value}` does not exist"
        ));
    }
    Ok(())
}

pub(super) fn file_name(value: &str) -> Result<String, String> {
    Path::new(value)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| format!("C++ input path `{value}` has no file name"))
}

pub(super) fn validate_identifier_path(kind: &str, value: &str) -> Result<(), String> {
    if value.split('.').all(is_identifier_segment) {
        Ok(())
    } else {
        Err(format!("{kind} `{value}` must be a dotted identifier path"))
    }
}

pub(super) fn validate_cpp_identifier_path(kind: &str, value: &str) -> Result<(), String> {
    if value.split("::").all(is_identifier_segment) {
        Ok(())
    } else {
        Err(format!("{kind} `{value}` must be a C++ identifier path"))
    }
}

pub(super) fn validate_upper_identifier(kind: &str, value: &str) -> Result<(), String> {
    if is_identifier_segment(value) && value.chars().next().is_some_and(char::is_uppercase) {
        Ok(())
    } else {
        Err(format!(
            "{kind} `{value}` must start with an uppercase letter"
        ))
    }
}

pub(super) fn validate_lower_identifier(kind: &str, value: &str) -> Result<(), String> {
    if is_identifier_segment(value) && value.chars().next().is_some_and(char::is_lowercase) {
        Ok(())
    } else {
        Err(format!(
            "{kind} `{value}` must start with a lowercase letter"
        ))
    }
}

pub(super) fn validate_cargo_package_name(value: &str) -> Result<(), String> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err("native binding package crate_name cannot be empty".into());
    };
    if !(first.is_ascii_lowercase() || first.is_ascii_digit())
        || !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        return Err(format!("invalid Cargo package name `{value}`"));
    }
    Ok(())
}

pub(super) fn is_identifier_segment(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub(super) fn module_source_path(module: &str) -> PathBuf {
    let mut path = PathBuf::from("src");
    for part in module.split('.') {
        path.push(part);
    }
    path.set_extension("terl");
    path
}

pub(super) fn module_docs_path(module: &str) -> PathBuf {
    PathBuf::from("docs").join(format!("{module}.md"))
}

pub(super) fn consumer_test_path(module: &str) -> PathBuf {
    let mut path = PathBuf::from("tests");
    let mut parts = module.split('.').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_some() {
            path.push(part);
        } else {
            path.push(format!("{part}Test.terl"));
        }
    }
    path
}

pub(super) fn refuse_non_empty_output(out_dir: &Path) -> Result<(), String> {
    if !out_dir.exists() {
        return Ok(());
    }
    if fs::read_dir(out_dir)
        .map_err(|err| {
            format!(
                "failed to read output directory `{}`: {err}",
                out_dir.display()
            )
        })?
        .next()
        .transpose()
        .map_err(|err| {
            format!(
                "failed to inspect output directory `{}`: {err}",
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

pub(super) fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create directory `{}`: {err}", parent.display()))?;
    }
    fs::copy(source, destination).map(|_| ()).map_err(|err| {
        format!(
            "failed to copy C++ input `{}` to `{}`: {err}",
            source.display(),
            destination.display()
        )
    })
}

pub(super) fn write_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create directory `{}`: {err}", parent.display()))?;
    }
    fs::write(path, contents)
        .map_err(|err| format!("failed to write generated file `{}`: {err}", path.display()))
}
