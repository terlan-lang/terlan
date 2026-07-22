
fn validate_owned_string_array_binding(
    function: &CAbiBindingFunction,
    symbol: &CSymbol,
) -> Result<(), String> {
    let outputs = symbol
        .parameters
        .iter()
        .filter(|parameter| parameter.owned_string_array.is_some())
        .count();
    if (function.returns == "List[String]" && outputs != 1)
        || (function.returns != "List[String]" && outputs != 0)
    {
        return Err(format!(
            "error[native_bindgen.c_owned_string_array_contract]: `{}` must map one owned string-array output to a List[String] return",
            function.name
        ));
    }
    Ok(())
}

fn validate_supported_c_scalar(c_type: &str) -> Result<(), String> {
    let normalized = c_type.trim().trim_start_matches("const ").trim();
    if is_builtin_c_type(normalized) || is_c_identifier(normalized) {
        Ok(())
    } else {
        Err(format!(
            "error[native_bindgen.unsupported_c_type]: `{c_type}` has no C ABI mapping"
        ))
    }
}

fn is_builtin_c_type(c_type: &str) -> bool {
    matches!(
        c_type,
        "void"
            | "bool"
            | "int8_t"
            | "uint8_t"
            | "int16_t"
            | "uint16_t"
            | "int32_t"
            | "uint32_t"
            | "int64_t"
            | "uint64_t"
            | "size_t"
            | "float"
            | "double"
            | "char"
    )
}

fn stable_shape_error(symbol: &CSymbol, shape: UnsupportedCShape) -> String {
    format!(
        "error[{}]: structured C symbol `{}` cannot be bound",
        skip_reason(shape),
        symbol.id
    )
}

fn collect_skipped_symbols(symbols: &[CSymbol]) -> Result<Vec<SkippedCSymbol>, String> {
    let mut skipped = symbols
        .iter()
        .filter(|symbol| symbol.status == CSymbolStatus::Unsupported)
        .map(|symbol| {
            let shape = symbol.unsupported_shape.ok_or_else(|| {
                format!(
                    "unsupported C symbol `{}` requires unsupported_shape",
                    symbol.id
                )
            })?;
            Ok(SkippedCSymbol {
                id: symbol.id.clone(),
                symbol: symbol.c_name.clone(),
                reason: skip_reason(shape).to_string(),
                detail: symbol.detail.clone().unwrap_or_default(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    skipped.sort();
    Ok(skipped)
}

fn skip_reason(shape: UnsupportedCShape) -> &'static str {
    match shape {
        UnsupportedCShape::PointerOwnershipUnknown => "native_bindgen.c_pointer_ownership_unknown",
        UnsupportedCShape::BorrowedLifetime => "native_bindgen.c_borrowed_lifetime",
        UnsupportedCShape::MissingDestructor => "native_bindgen.c_missing_destructor",
        UnsupportedCShape::UnsupportedCallback => "native_bindgen.c_unsupported_callback",
        UnsupportedCShape::UnsupportedVariadicFunction => {
            "native_bindgen.c_unsupported_variadic_function"
        }
        UnsupportedCShape::UnsupportedUnion => "native_bindgen.c_unsupported_union",
        UnsupportedCShape::UnsupportedBitfield => "native_bindgen.c_unsupported_bitfield",
        UnsupportedCShape::AbiVersionMissing => "native_bindgen.c_abi_version_missing",
        UnsupportedCShape::ThreadLocalError => "native_bindgen.c_thread_local_error",
    }
}

fn validate_binding_roles(manifest: &CAbiBindingManifest) -> Result<(), String> {
    let types = binding_types(manifest);
    if types.is_empty() {
        return Err("C ABI package requires at least one opaque handle type".into());
    }
    let mut type_names = BTreeSet::new();
    for (_, ty) in &types {
        if !type_names.insert(ty.name.as_str()) {
            return Err(format!("duplicate C ABI opaque handle type `{}`", ty.name));
        }
    }
    let constructor_count = manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.role == CAbiFunctionRole::Constructor)
        .count();
    if constructor_count == 0 {
        return Err("C ABI package requires at least one `constructor` function; found 0".into());
    }
    for (_, ty) in types {
        let producers = manifest
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .filter(|function| {
                function.returns == ty.name
                    && matches!(
                        function.role,
                        CAbiFunctionRole::Constructor
                            | CAbiFunctionRole::ImmutableMethod
                            | CAbiFunctionRole::MutableMethod
                            | CAbiFunctionRole::FreeFunction
                    )
            })
            .count();
        if producers == 0 {
            return Err(format!(
                "C ABI opaque handle type `{}` requires at least one producer",
                ty.name
            ));
        }
        let disposers = manifest
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .filter(|function| {
                function.role == CAbiFunctionRole::Dispose
                    && function.args.len() == 1
                    && function.args[0].ty == ty.name
            })
            .count();
        if disposers != 1 {
            return Err(format!(
                "C ABI opaque handle type `{}` requires exactly one `dispose` function; found {disposers}",
                ty.name
            ));
        }
    }
    Ok(())
}

fn validate_dispatcher_binding(
    function: &CAbiBindingFunction,
    dispatcher: &CDispatcherBinding,
    call_symbol: &CSymbol,
    symbols: &BTreeMap<&str, &CSymbol>,
    aliases: &BTreeMap<String, String>,
) -> Result<(), String> {
    let family = "native_bindgen.c_dispatcher_contract";
    let fail = |detail: &str| -> String {
        format!(
            "error[{family}]: dispatcher binding `{}` {detail}",
            function.name
        )
    };
    if function.role != CAbiFunctionRole::ImmutableMethod
        || function.args.is_empty()
        || function.args[0].ty == "Int"
        || function.returns != function.args[0].ty
        || function.args[1..].iter().any(|argument| {
            argument.ty != "Int"
                && argument.ty != "Float"
                && argument.ty != "Bool"
                && argument.ty != "List[Int]"
                && argument.ty != function.args[0].ty
        })
    {
        return Err(fail(
            "must be an immutable same-handle method with only matching handle, Int, Float, Bool, or List[Int] arguments",
        ));
    }
    if dispatcher.operator_name.is_empty()
        || !dispatcher.operator_name.contains("::")
        || dispatcher.operator_name.contains('\0')
        || dispatcher.overload_name.contains('\0')
    {
        return Err(fail(
            "requires unambiguous NUL-free operator and overload names",
        ));
    }
    parse_dispatcher_abi_version(&dispatcher.extension_abi_version)
        .map_err(|detail| fail(&detail))?;
    if dispatcher.stack.is_empty() {
        return Err(fail("requires a non-empty StableIValue stack"));
    }
    let copied = dispatcher
        .stack
        .iter()
        .filter_map(|value| match value {
            CDispatcherStackValue::OwnedHandleCopy { argument }
            | CDispatcherStackValue::OwnedOptionalHandleCopy { argument } => {
                Some(argument.as_str())
            }
            CDispatcherStackValue::IntArgument { .. }
            | CDispatcherStackValue::FloatArgument { .. }
            | CDispatcherStackValue::BoolArgument { .. }
            | CDispatcherStackValue::OwnedOptionalIntArgument { .. }
            | CDispatcherStackValue::OwnedIntListArgument { .. }
            | CDispatcherStackValue::OwnedStringLiteral { .. }
            | CDispatcherStackValue::Null
            | CDispatcherStackValue::Unsupported => None,
        })
        .collect::<Vec<_>>();
    let expected_handles = function
        .args
        .iter()
        .filter(|argument| argument.ty == function.args[0].ty)
        .map(|argument| argument.name.as_str())
        .collect::<BTreeSet<_>>();
    if copied.len() != expected_handles.len()
        || copied.iter().copied().collect::<BTreeSet<_>>() != expected_handles
    {
        return Err(fail(
            "must encode every declared handle exactly once as owned_handle_copy or owned_optional_handle_copy",
        ));
    }
    let expected_ints = function
        .args
        .iter()
        .filter(|argument| argument.ty == "Int")
        .map(|argument| argument.name.as_str())
        .collect::<BTreeSet<_>>();
    let stack_ints = dispatcher
        .stack
        .iter()
        .filter_map(|value| match value {
            CDispatcherStackValue::IntArgument { argument }
            | CDispatcherStackValue::OwnedOptionalIntArgument { argument } => {
                Some(argument.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if stack_ints.len() != expected_ints.len()
        || stack_ints.iter().copied().collect::<BTreeSet<_>>() != expected_ints
    {
        return Err(fail(
            "must encode every declared Int argument exactly once as int_argument or owned_optional_int_argument",
        ));
    }
    let expected_floats = function
        .args
        .iter()
        .filter(|argument| argument.ty == "Float")
        .map(|argument| argument.name.as_str())
        .collect::<BTreeSet<_>>();
    let stack_floats = dispatcher
        .stack
        .iter()
        .filter_map(|value| match value {
            CDispatcherStackValue::FloatArgument { argument } => Some(argument.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if stack_floats.len() != expected_floats.len()
        || stack_floats.iter().copied().collect::<BTreeSet<_>>() != expected_floats
    {
        return Err(fail(
            "must encode every declared Float argument exactly once as float_argument",
        ));
    }
    let expected_bools = function
        .args
        .iter()
        .filter(|argument| argument.ty == "Bool")
        .map(|argument| argument.name.as_str())
        .collect::<BTreeSet<_>>();
    let stack_bools = dispatcher
        .stack
        .iter()
        .filter_map(|value| match value {
            CDispatcherStackValue::BoolArgument { argument } => Some(argument.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if stack_bools.len() != expected_bools.len()
        || stack_bools.iter().copied().collect::<BTreeSet<_>>() != expected_bools
    {
        return Err(fail(
            "must encode every declared Bool argument exactly once as bool_argument",
        ));
    }
    let expected_int_lists = function
        .args
        .iter()
        .filter(|argument| argument.ty == "List[Int]")
        .map(|argument| argument.name.as_str())
        .collect::<BTreeSet<_>>();
    let stack_int_lists = dispatcher
        .stack
        .iter()
        .filter_map(|value| match value {
            CDispatcherStackValue::OwnedIntListArgument { argument } => Some(argument.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if stack_int_lists.len() != expected_int_lists.len()
        || stack_int_lists.iter().copied().collect::<BTreeSet<_>>() != expected_int_lists
    {
        return Err(fail(
            "must encode every declared List[Int] argument exactly once as owned_int_list_argument",
        ));
    }
    if dispatcher
        .stack
        .iter()
        .any(|value| matches!(value, CDispatcherStackValue::Unsupported))
    {
        return Err(fail("contains an unsupported StableIValue stack kind"));
    }
    let has_owned_optional = dispatcher.stack.iter().any(|value| {
        matches!(
            value,
            CDispatcherStackValue::OwnedOptionalIntArgument { .. }
                | CDispatcherStackValue::OwnedOptionalHandleCopy { .. }
        )
    });
    if has_owned_optional {
        let allocator_id = dispatcher
            .optional_value_allocator_symbol
            .as_deref()
            .ok_or_else(|| fail("requires optional_value_allocator_symbol for owned optionals"))?;
        let destructor_id = dispatcher
            .optional_value_destructor_symbol
            .as_deref()
            .ok_or_else(|| fail("requires optional_value_destructor_symbol for owned optionals"))?;
        let allocator = symbols
            .get(allocator_id)
            .ok_or_else(|| fail("references an unknown optional_value_allocator_symbol"))?;
        let destructor = symbols
            .get(destructor_id)
            .ok_or_else(|| fail("references an unknown optional_value_destructor_symbol"))?;
        let allocator_parameters = allocator
            .parameters
            .iter()
            .map(|parameter| resolve_c_type(&parameter.c_type, aliases))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|detail| fail(&detail))?;
        let destructor_parameters = destructor
            .parameters
            .iter()
            .map(|parameter| resolve_c_type(&parameter.c_type, aliases))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|detail| fail(&detail))?;
        if allocator.status != CSymbolStatus::Bind
            || allocator.kind != CSymbolKind::Function
            || allocator.error_model != Some(CErrorModel::StatusCode)
            || allocator_parameters != ["uint64_t **"]
            || allocator.parameters[0].direction != Some(CParameterDirection::Output)
            || allocator.parameters[0].ownership != Some(CParameterOwnership::TransferFull)
        {
            return Err(fail(
                "requires a status-returning uint64_t optional-value allocator",
            ));
        }
        if destructor.status != CSymbolStatus::Bind
            || destructor.kind != CSymbolKind::Function
            || destructor.error_model != Some(CErrorModel::StatusCode)
            || destructor_parameters != ["uint64_t *"]
            || destructor.parameters[0].direction != Some(CParameterDirection::Input)
            || destructor.parameters[0].ownership != Some(CParameterOwnership::TransferFull)
        {
            return Err(fail(
                "requires a status-returning uint64_t optional-value destructor",
            ));
        }
    } else if dispatcher.optional_value_allocator_symbol.is_some()
        || dispatcher.optional_value_destructor_symbol.is_some()
    {
        return Err(fail(
            "must not declare optional-value symbols without an owned optional stack value",
        ));
    }
    let has_owned_int_list = dispatcher
        .stack
        .iter()
        .any(|value| matches!(value, CDispatcherStackValue::OwnedIntListArgument { .. }));
    if has_owned_int_list {
        let allocator_id = dispatcher
            .list_allocator_symbol
            .as_deref()
            .ok_or_else(|| fail("requires list_allocator_symbol for owned lists"))?;
        let push_id = dispatcher
            .list_push_symbol
            .as_deref()
            .ok_or_else(|| fail("requires list_push_symbol for owned lists"))?;
        let destructor_id = dispatcher
            .list_destructor_symbol
            .as_deref()
            .ok_or_else(|| fail("requires list_destructor_symbol for owned lists"))?;
        let allocator = symbols
            .get(allocator_id)
            .ok_or_else(|| fail("references an unknown list_allocator_symbol"))?;
        let push = symbols
            .get(push_id)
            .ok_or_else(|| fail("references an unknown list_push_symbol"))?;
        let destructor = symbols
            .get(destructor_id)
            .ok_or_else(|| fail("references an unknown list_destructor_symbol"))?;
        let parameters = |symbol: &CSymbol| {
            symbol
                .parameters
                .iter()
                .map(|parameter| resolve_c_type(&parameter.c_type, aliases))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|detail| fail(&detail))
        };
        if allocator.status != CSymbolStatus::Bind
            || allocator.kind != CSymbolKind::Function
            || allocator.error_model != Some(CErrorModel::StatusCode)
            || parameters(allocator)? != ["size_t", "void **"]
            || allocator.parameters[0].direction != Some(CParameterDirection::Input)
            || allocator.parameters[0].ownership != Some(CParameterOwnership::Value)
            || allocator.parameters[1].direction != Some(CParameterDirection::Output)
            || allocator.parameters[1].ownership != Some(CParameterOwnership::TransferFull)
        {
            return Err(fail(
                "requires a status-returning size_t/void** list allocator",
            ));
        }
        if push.status != CSymbolStatus::Bind
            || push.kind != CSymbolKind::Function
            || push.error_model != Some(CErrorModel::StatusCode)
            || parameters(push)? != ["void *", "uint64_t"]
            || push.parameters[0].direction != Some(CParameterDirection::Input)
            || push.parameters[0].ownership != Some(CParameterOwnership::BorrowedCall)
            || push.parameters[1].direction != Some(CParameterDirection::Input)
            || push.parameters[1].ownership != Some(CParameterOwnership::Value)
        {
            return Err(fail(
                "requires a status-returning void*/uint64_t list push symbol",
            ));
        }
        if destructor.status != CSymbolStatus::Bind
            || destructor.kind != CSymbolKind::Function
            || destructor.error_model != Some(CErrorModel::StatusCode)
            || parameters(destructor)? != ["void *"]
            || destructor.parameters[0].direction != Some(CParameterDirection::Input)
            || destructor.parameters[0].ownership != Some(CParameterOwnership::TransferFull)
        {
            return Err(fail("requires a status-returning void* list destructor"));
        }
    } else if dispatcher.list_allocator_symbol.is_some()
        || dispatcher.list_push_symbol.is_some()
        || dispatcher.list_destructor_symbol.is_some()
    {
        return Err(fail(
            "must not declare list symbols without an owned list stack value",
        ));
    }
    let has_owned_string = dispatcher
        .stack
        .iter()
        .any(|value| matches!(value, CDispatcherStackValue::OwnedStringLiteral { .. }));
    if has_owned_string {
        let allocator_id = dispatcher
            .string_allocator_symbol
            .as_deref()
            .ok_or_else(|| fail("requires string_allocator_symbol for owned strings"))?;
        let destructor_id = dispatcher
            .string_destructor_symbol
            .as_deref()
            .ok_or_else(|| fail("requires string_destructor_symbol for owned strings"))?;
        let allocator = symbols
            .get(allocator_id)
            .ok_or_else(|| fail("references an unknown string_allocator_symbol"))?;
        let destructor = symbols
            .get(destructor_id)
            .ok_or_else(|| fail("references an unknown string_destructor_symbol"))?;
        let parameters = |symbol: &CSymbol| {
            symbol
                .parameters
                .iter()
                .map(|parameter| resolve_c_type(&parameter.c_type, aliases))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|detail| fail(&detail))
        };
        let allocator_parameters = parameters(allocator)?;
        let destructor_parameters = parameters(destructor)?;
        if allocator.status != CSymbolStatus::Bind
            || allocator.kind != CSymbolKind::Function
            || allocator.error_model != Some(CErrorModel::StatusCode)
            || allocator_parameters.len() != 3
            || allocator_parameters[0] != "const char *"
            || allocator_parameters[1] != "size_t"
            || c_pointer_base(&allocator_parameters[2]) != "void"
            || allocator_parameters[2].matches('*').count() != 2
            || allocator.parameters[0].direction != Some(CParameterDirection::Input)
            || allocator.parameters[0].ownership != Some(CParameterOwnership::BorrowedCall)
            || allocator.parameters[1].direction != Some(CParameterDirection::Input)
            || allocator.parameters[1].ownership != Some(CParameterOwnership::Value)
            || allocator.parameters[2].direction != Some(CParameterDirection::Output)
            || allocator.parameters[2].ownership != Some(CParameterOwnership::TransferFull)
        {
            return Err(fail(
                "requires a status-returning const-char*/size_t/void** string allocator",
            ));
        }
        if destructor.status != CSymbolStatus::Bind
            || destructor.kind != CSymbolKind::Function
            || destructor.error_model != Some(CErrorModel::StatusCode)
            || destructor_parameters.len() != 1
            || c_pointer_base(&destructor_parameters[0]) != "void"
            || destructor_parameters[0].matches('*').count() != 1
            || destructor.parameters[0].direction != Some(CParameterDirection::Input)
            || destructor.parameters[0].ownership != Some(CParameterOwnership::TransferFull)
        {
            return Err(fail("requires a status-returning void* string destructor"));
        }
    } else if dispatcher.string_allocator_symbol.is_some()
        || dispatcher.string_destructor_symbol.is_some()
    {
        return Err(fail(
            "must not declare string symbols without an owned string stack value",
        ));
    }
    if dispatcher.output.kind != CDispatcherOutputKind::OwnedHandle || dispatcher.output.index != 0
    {
        return Err(fail("requires one owned_handle output in stack slot zero"));
    }
    let duplicate = symbols
        .get(dispatcher.duplicate_handle_symbol.as_str())
        .ok_or_else(|| fail("references an unknown duplicate_handle_symbol"))?;
    if duplicate.status != CSymbolStatus::Bind
        || duplicate.kind != CSymbolKind::Function
        || duplicate.error_model != Some(CErrorModel::StatusCode)
        || duplicate.parameters.len() != 2
        || duplicate.parameters[0].direction != Some(CParameterDirection::Input)
        || duplicate.parameters[0].ownership != Some(CParameterOwnership::BorrowedCall)
        || duplicate.parameters[1].direction != Some(CParameterDirection::Output)
        || duplicate.parameters[1].ownership != Some(CParameterOwnership::TransferFull)
    {
        return Err(fail(
            "requires a status-returning borrowed-handle to owned-handle duplicate symbol",
        ));
    }
    let duplicate_input =
        resolve_c_type(&duplicate.parameters[0].c_type, aliases).map_err(|detail| fail(&detail))?;
    let duplicate_output =
        resolve_c_type(&duplicate.parameters[1].c_type, aliases).map_err(|detail| fail(&detail))?;
    if duplicate_input.matches('*').count() != 1
        || duplicate_output.matches('*').count() != 2
        || c_pointer_base(&duplicate_input) != c_pointer_base(&duplicate_output)
    {
        return Err(fail(
            "requires duplicate input and output parameters for the same opaque handle",
        ));
    }
    let call_parameters = call_symbol
        .parameters
        .iter()
        .map(|parameter| resolve_c_type(&parameter.c_type, aliases))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|detail| fail(&detail))?;
    if call_symbol.error_model != Some(CErrorModel::StatusCode)
        || call_symbol.parameters.len() != 4
        || call_parameters != ["const char *", "const char *", "uint64_t *", "uint64_t"]
        || call_symbol.parameters[0].direction != Some(CParameterDirection::Input)
        || call_symbol.parameters[0].ownership != Some(CParameterOwnership::BorrowedCall)
        || call_symbol.parameters[1].direction != Some(CParameterDirection::Input)
        || call_symbol.parameters[1].ownership != Some(CParameterOwnership::BorrowedCall)
        || call_symbol.parameters[2].direction != Some(CParameterDirection::InOut)
        || call_symbol.parameters[2].ownership != Some(CParameterOwnership::BorrowedCall)
        || call_symbol.parameters[3].direction != Some(CParameterDirection::Input)
        || call_symbol.parameters[3].ownership != Some(CParameterOwnership::Value)
    {
        return Err(fail(
            "requires the reviewed torch_call_dispatcher StableIValue signature",
        ));
    }
    Ok(())
}

fn parse_dispatcher_abi_version(value: &str) -> Result<u64, String> {
    let digits = value
        .strip_prefix("0x")
        .ok_or_else(|| "requires extension_abi_version as a 0x-prefixed u64".to_string())?;
    if digits.len() != 16 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("requires extension_abi_version as exactly 16 hexadecimal digits".to_string());
    }
    let parsed = u64::from_str_radix(digits, 16)
        .map_err(|_| "has an invalid extension_abi_version".to_string())?;
    if parsed == 0 {
        return Err("requires a non-zero extension_abi_version".to_string());
    }
    Ok(parsed)
}

fn validate_c_inputs(metadata: &CMetadata, input_dir: &Path) -> Result<(), String> {
    if let Some(link) = &metadata.external_link {
        match (&link.root_env, &link.pkg_config) {
            (Some(root_env), None) => {
                if !is_environment_variable(root_env) {
                    return Err(format!(
                        "external C root environment variable `{root_env}` is invalid"
                    ));
                }
                if link.library_dirs.is_empty() || link.libraries.is_empty() {
                    return Err(
                        "environment-rooted external C metadata requires library directories and libraries"
                            .into(),
                    );
                }
            }
            (None, Some(pkg_config)) => {
                if !is_link_library_name(&pkg_config.package) {
                    return Err(format!(
                        "external C pkg-config package `{}` is invalid",
                        pkg_config.package
                    ));
                }
                if pkg_config
                    .min_version
                    .as_deref()
                    .is_some_and(|version| !is_version_requirement(version))
                {
                    return Err("external C pkg-config min_version is invalid".into());
                }
                if !link.library_dirs.is_empty()
                    || !link.libraries.is_empty()
                    || !link.runtime_library_dirs.is_empty()
                {
                    return Err(
                        "pkg-config external C metadata must obtain library paths and names from pkg-config"
                            .into(),
                    );
                }
            }
            (Some(_), Some(_)) => {
                return Err(
                    "external C metadata must choose exactly one of root_env or pkg_config".into(),
                );
            }
            (None, None) => {
                return Err(
                    "external C metadata requires exactly one of root_env or pkg_config".into(),
                );
            }
        }
        validate_relative_metadata_path(&metadata.header)?;
        for path in link
            .include_dirs
            .iter()
            .chain(&link.library_dirs)
            .chain(&link.runtime_library_dirs)
        {
            validate_relative_metadata_path(path)?;
        }
        for library in &link.libraries {
            if !is_link_library_name(library) {
                return Err(format!("external C library name `{library}` is invalid"));
            }
        }
        for source in &metadata.sources {
            validate_input_path(input_dir, source)?;
        }
        for header in &metadata.headers {
            validate_input_path(input_dir, header)?;
        }
        return Ok(());
    }

    validate_input_path(input_dir, &metadata.header)?;
    if metadata.sources.is_empty() {
        return Err("structured C metadata must declare sources or external_link".into());
    }
    for source in &metadata.sources {
        validate_input_path(input_dir, source)?;
    }
    for header in &metadata.headers {
        validate_input_path(input_dir, header)?;
    }
    Ok(())
}

fn validate_rust_extension(package: &CAbiBindingPackage, input_dir: &Path) -> Result<(), String> {
    let Some(extension) = &package.rust_extension else {
        return Ok(());
    };
    validate_input_path(input_dir, &extension.source)?;
    if Path::new(&extension.source)
        .extension()
        .and_then(|value| value.to_str())
        != Some("rs")
    {
        return Err("C ABI package rust_extension source must be a Rust source file".into());
    }
    for (name, version) in &extension.dependencies {
        validate_cargo_package_name(name)?;
        if !is_pinned_cargo_version(version) {
            return Err(format!(
                "C ABI package Rust dependency `{name}` must use an exact stable x.y.z version; found `{version}`"
            ));
        }
    }
    Ok(())
}

fn validate_c_aliases(metadata: &CMetadata) -> Result<(), String> {
    for (name, target) in &metadata.aliases {
        if !is_c_identifier(name) {
            return Err(format!("C alias name `{name}` is invalid"));
        }
        let resolved = resolve_c_type(target, &metadata.aliases)?;
        let base = c_pointer_base(&resolved);
        if !is_builtin_c_type(base) && !is_c_identifier(base) {
            return Err(format!(
                "C alias `{name}` resolves to unsupported type `{target}`"
            ));
        }
    }
    Ok(())
}

fn copy_c_inputs(metadata: &CMetadata, input_dir: &Path, out_dir: &Path) -> Result<(), String> {
    if metadata.external_link.is_none() {
        let header_name = file_name(&metadata.header)?;
        copy_file(
            &input_dir.join(&metadata.header),
            &out_dir.join("native/rust/include").join(header_name),
        )?;
    }
    for header in &metadata.headers {
        copy_file(
            &input_dir.join(header),
            &out_dir.join("native/rust/c").join(file_name(header)?),
        )?;
    }
    for source in &metadata.sources {
        copy_file(
            &input_dir.join(source),
            &out_dir.join("native/rust/c").join(file_name(source)?),
        )?;
    }
    Ok(())
}

fn copy_rust_extension(
    package: &CAbiBindingPackage,
    input_dir: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let Some(extension) = &package.rust_extension else {
        return Ok(());
    };
    copy_file(
        &input_dir.join(&extension.source),
        &out_dir.join("native/rust/src/package_extension.rs"),
    )
}

fn render_terlan_manifest(manifest: &CAbiBindingManifest) -> String {
    let package_name = manifest
        .package
        .name
        .as_deref()
        .unwrap_or(&manifest.package.crate_name);
    let package_version = manifest.package.version.as_deref().unwrap_or("0.0.0");
    format!(
        "[package]\nname = {:?}\nversion = {:?}\nnamespace = {:?}\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n\n[native.rust]\ncrate = {:?}\npath = \"native/rust\"\nhelper = \"native-boundary-helper\"\nhelper_env = \"TERLAN_NATIVE_BOUNDARY_HELPER_PATH\"\n",
        package_name,
        package_version,
        manifest.package.namespace,
        manifest.package.crate_name
    )
}

fn render_module_source(module: &CAbiBindingModule) -> String {
    let mut source = format!(
        "/**\n * {}\n */\n\nmodule {}.\n\n",
        module.documentation, module.module
    );
    for ty in &module.types {
        source.push_str(&format!(
            "/** {} */\npub opaque type {}.\n\n",
            ty.documentation, ty.name
        ));
    }
    for function in &module.functions {
        source.push_str(&format!(
            "/** {} */\n@compiler.native {{{}}}\npub {}({}): {} -> native.\n\n",
            function.documentation,
            function.operation,
            function.name,
            render_args(&function.args),
            function.returns
        ));
    }
    source
}

fn render_module_docs(module: &CAbiBindingModule, symbols: &BTreeMap<&str, &CSymbol>) -> String {
    let mut docs = format!("# {}\n\n{}\n\n", module.module, module.documentation);
    for function in &module.functions {
        docs.push_str(&format!(
            "## `{}`\n\n{}\n\n",
            function.name, function.documentation
        ));
        if let Some(symbol) = symbols.get(function.c_symbol.as_str()) {
            docs.push_str(&format!("- C symbol: `{}`\n", symbol.c_name));
        }
        docs.push_str(&format!(
            "- NativeBoundary operation: `{}`\n- Ownership: `{}`\n\n",
            function.operation,
            resource_policy_name(&function.resource)
        ));
    }
    docs
}
