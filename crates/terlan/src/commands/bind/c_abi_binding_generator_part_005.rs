
fn render_safe_wrapper(
    manifest: &CAbiBindingManifest,
    function: &CAbiBindingFunction,
    symbol: &CSymbol,
    symbols: &BTreeMap<&str, &CSymbol>,
    record: &CSymbol,
    ty: &CAbiBindingType,
    aliases: &BTreeMap<String, String>,
    inside_impl: bool,
) -> Result<String, String> {
    let indent = if inside_impl { "    " } else { "" };
    let resource_types = binding_types(manifest);
    let resource_by_c_name = resource_types
        .iter()
        .map(|(_, resource_ty)| {
            let resource_record = symbols
                .get(resource_ty.c_symbol.as_str())
                .copied()
                .ok_or_else(|| format!("unknown C record `{}`", resource_ty.c_symbol))?;
            Ok((
                resource_record.c_name.as_str(),
                *resource_ty,
                resource_record,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut rust_args = Vec::new();
    let mut receiver_skipped = false;
    for argument in &function.args {
        if argument.ty == ty.name
            && inside_impl
            && !receiver_skipped
            && matches!(
                function.role,
                CAbiFunctionRole::ImmutableMethod | CAbiFunctionRole::MutableMethod
            )
        {
            receiver_skipped = true;
            continue;
        }
        let rust_ty = match argument.ty.as_str() {
            "Int" => "i64",
            "Float" => "f64",
            "Bool" => "bool",
            "String" => "&str",
            "List[Int]" => "&[i64]",
            "List[Float]" => "&[f64]",
            "List[Bool]" => "&[bool]",
            value if value == ty.name && inside_impl => "&Self",
            value if binding_type(manifest, value).is_some() => {
                rust_args.push(format!("{}: &{value}", argument.name));
                continue;
            }
            value => {
                return Err(format!(
                    "error[native_bindgen.unsupported_terlan_type]: `{value}` in `{}`",
                    function.name
                ));
            }
        };
        rust_args.push(format!("{}: {rust_ty}", argument.name));
    }
    let receiver = match function.role {
        CAbiFunctionRole::ImmutableMethod => Some("&self"),
        CAbiFunctionRole::MutableMethod => Some("&mut self"),
        _ => None,
    };
    let mut signature_args = receiver.into_iter().map(str::to_string).collect::<Vec<_>>();
    signature_args.extend(rust_args);
    let return_ty = match function.returns.as_str() {
        value if value == ty.name => "Self",
        value if binding_type(manifest, value).is_some() => value,
        "Int" => "i64",
        "Float" => "f64",
        "Bool" => "bool",
        "String" => "String",
        "List[Int]" => "Vec<i64>",
        "List[Float]" => "Vec<f64>",
        "List[Bool]" => "Vec<bool>",
        "List[String]" => "Vec<String>",
        "Unit" => "()",
        value => {
            return Err(format!(
                "error[native_bindgen.unsupported_terlan_type]: return `{value}` in `{}`",
                function.name
            ));
        }
    };
    let fallible = symbol.error_model == Some(CErrorModel::StatusCode);
    let public_return = if fallible {
        format!("Result<{return_ty}, CAbiError>")
    } else {
        return_ty.to_string()
    };
    let mut rendered = format!(
        "{indent}pub fn {}({}) -> {public_return} {{\n",
        function.name,
        signature_args.join(", ")
    );

    if let Some(dispatcher) = &function.dispatcher {
        rendered.push_str(&render_dispatcher_wrapper_body(
            function, dispatcher, symbol, symbols, record, indent,
        )?);
        rendered.push_str(&format!("{indent}}}\n\n"));
        return Ok(rendered);
    }

    let mut call_args = Vec::new();
    let mut scalar_output: Option<(String, String)> = None;
    let mut array_output: Option<(String, String)> = None;
    let mut owned_string_output: Option<(String, String, String)> = None;
    let mut owned_array_output: Option<(String, String, String, COwnedArrayElement)> = None;
    let mut owned_string_array_output: Option<(String, String, String, String)> = None;
    let mut handle_output: Option<&CAbiBindingType> = None;
    let input_array_lengths = symbol
        .parameters
        .iter()
        .filter_map(|parameter| {
            parameter
                .input_array
                .as_ref()
                .map(|array| (array.length_parameter.as_str(), parameter.name.as_str()))
        })
        .collect::<BTreeMap<_, _>>();
    let owned_string_lengths = symbol
        .parameters
        .iter()
        .filter_map(|parameter| {
            parameter
                .owned_string
                .as_ref()
                .map(|string| (string.length_parameter.as_str(), parameter.name.as_str()))
        })
        .collect::<BTreeMap<_, _>>();
    let owned_array_lengths = symbol
        .parameters
        .iter()
        .filter_map(|parameter| {
            parameter
                .owned_array
                .as_ref()
                .map(|array| (array.length_parameter.as_str(), parameter.name.as_str()))
        })
        .collect::<BTreeMap<_, _>>();
    let owned_string_array_lengths = symbol
        .parameters
        .iter()
        .filter_map(|parameter| {
            parameter
                .owned_string_array
                .as_ref()
                .map(|array| (array.lengths_parameter.as_str(), parameter.name.as_str()))
        })
        .collect::<BTreeMap<_, _>>();
    let owned_string_array_counts = symbol
        .parameters
        .iter()
        .filter_map(|parameter| {
            parameter
                .owned_string_array
                .as_ref()
                .map(|array| (array.count_parameter.as_str(), parameter.name.as_str()))
        })
        .collect::<BTreeMap<_, _>>();
    for parameter in &symbol.parameters {
        let resolved = resolve_c_type(&parameter.c_type, aliases)?;
        let base = c_pointer_base(&resolved);
        let handle_mapping = resource_by_c_name
            .iter()
            .find(|(c_name, _, _)| *c_name == base);
        let is_handle = handle_mapping.is_some();
        if let Some(fixed) = &parameter.fixed {
            match fixed {
                CFixedInput::Null => call_args.push("std::ptr::null_mut()".to_string()),
                CFixedInput::Int32 { value } if resolved.contains('*') => {
                    rendered.push_str(&format!(
                        "{indent}    let mut fixed_{}: i32 = {value};\n",
                        parameter.name
                    ));
                    call_args.push(format!("&mut fixed_{}", parameter.name));
                }
                CFixedInput::Int32 { value } => call_args.push(format!("{value}i32")),
            }
            continue;
        }
        if parameter.input_array.is_some() {
            if base == "uint8_t" {
                rendered.push_str(&format!(
                    "{indent}    let {}_bytes = {}.iter().copied().map(u8::from).collect::<Vec<_>>();\n",
                    parameter.name, parameter.name
                ));
            }
            rendered.push_str(&format!(
                "{indent}    let {}_length = i64::try_from({}.len()).map_err(|_| CAbiError {{ operation: {:?}, status: -2 }})?;\n",
                parameter.name, parameter.name, function.operation
            ));
            if base == "uint8_t" {
                call_args.push(format!("{}_bytes.as_ptr()", parameter.name));
            } else {
                call_args.push(format!("{}.as_ptr()", parameter.name));
            }
            continue;
        }
        if let Some(array_name) = input_array_lengths.get(parameter.name.as_str()) {
            call_args.push(format!("{array_name}_length"));
            continue;
        }
        if let Some(string_name) = owned_string_lengths.get(parameter.name.as_str()) {
            call_args.push(format!("&mut out_{}_length", string_name));
            continue;
        }
        if let Some(array_name) = owned_array_lengths.get(parameter.name.as_str()) {
            call_args.push(format!("&mut out_{}_length", array_name));
            continue;
        }
        if let Some(array_name) = owned_string_array_lengths.get(parameter.name.as_str()) {
            call_args.push(format!("&mut out_{}_lengths", array_name));
            continue;
        }
        if let Some(array_name) = owned_string_array_counts.get(parameter.name.as_str()) {
            call_args.push(format!("&mut out_{}_count", array_name));
            continue;
        }
        if let Some(string) = &parameter.owned_string {
            let destructor = symbols
                .get(string.destructor_symbol.as_str())
                .expect("validated owned-string destructor");
            rendered.push_str(&format!(
                "{indent}    let mut out_{}: *mut std::ffi::c_char = std::ptr::null_mut();\n{indent}    let mut out_{}_length: usize = 0;\n",
                parameter.name, parameter.name
            ));
            call_args.push(format!("&mut out_{}", parameter.name));
            owned_string_output = Some((
                format!("out_{}", parameter.name),
                format!("out_{}_length", parameter.name),
                destructor.c_name.clone(),
            ));
            continue;
        }
        if let Some(array) = &parameter.owned_array {
            let destructor = symbols
                .get(array.destructor_symbol.as_str())
                .expect("validated owned-array destructor");
            let rust_element = array.element.rust_element();
            rendered.push_str(&format!(
                "{indent}    let mut out_{}: *mut {rust_element} = std::ptr::null_mut();\n{indent}    let mut out_{}_length: usize = 0;\n",
                parameter.name, parameter.name
            ));
            call_args.push(format!("&mut out_{}", parameter.name));
            owned_array_output = Some((
                format!("out_{}", parameter.name),
                format!("out_{}_length", parameter.name),
                destructor.c_name.clone(),
                array.element,
            ));
            continue;
        }
        if let Some(array) = &parameter.owned_string_array {
            let destructor = symbols
                .get(array.destructor_symbol.as_str())
                .expect("validated owned-string-array destructor");
            rendered.push_str(&format!(
                "{indent}    let mut out_{}: *mut *mut std::ffi::c_char = std::ptr::null_mut();\n{indent}    let mut out_{}_lengths: *mut usize = std::ptr::null_mut();\n{indent}    let mut out_{}_count: usize = 0;\n",
                parameter.name, parameter.name, parameter.name
            ));
            call_args.push(format!("&mut out_{}", parameter.name));
            owned_string_array_output = Some((
                format!("out_{}", parameter.name),
                format!("out_{}_lengths", parameter.name),
                format!("out_{}_count", parameter.name),
                destructor.c_name.clone(),
            ));
            continue;
        }
        if resolved == "const char *"
            && parameter.direction == Some(CParameterDirection::Input)
            && parameter.ownership == Some(CParameterOwnership::BorrowedCall)
        {
            let argument = function
                .args
                .iter()
                .find(|argument| argument.name == parameter.name && argument.ty == "String")
                .ok_or_else(|| {
                    format!(
                        "error[native_bindgen.unsupported_wrapper_shape]: C string parameter `{}` in `{}` requires a matching String argument",
                        parameter.name, function.name
                    )
                })?;
            rendered.push_str(&format!(
                "{indent}    let {}_c = std::ffi::CString::new({}.as_bytes()).map_err(|_| CAbiError {{ operation: {:?}, status: -3 }})?;\n",
                parameter.name, argument.name, function.operation
            ));
            call_args.push(format!("{}_c.as_ptr()", parameter.name));
            continue;
        }
        match (parameter.direction, parameter.ownership, is_handle) {
            (Some(CParameterDirection::Input), _, true) => {
                let (_, parameter_ty, _) = handle_mapping.expect("matched resource handle");
                let argument = function.args.iter().find(|argument| {
                    argument.name == parameter.name && argument.ty == parameter_ty.name
                }).ok_or_else(|| {
                    format!(
                        "error[native_bindgen.unsupported_wrapper_shape]: C handle parameter `{}` in `{}` has no Terlan handle argument",
                        parameter.name, function.name
                    )
                })?;
                if inside_impl
                    && function.args.first().is_some_and(|receiver| {
                        receiver.name == argument.name && receiver.ty == ty.name
                    })
                {
                    call_args.push("self.raw.as_ptr()".to_string());
                } else {
                    call_args.push(format!("{}.raw.as_ptr()", argument.name));
                }
            }
            (Some(CParameterDirection::Output), Some(CParameterOwnership::TransferFull), true) => {
                let (_, output_ty, output_record) =
                    handle_mapping.expect("matched resource handle");
                if function.returns != output_ty.name {
                    return Err(format!(
                        "error[native_bindgen.unsupported_wrapper_shape]: C handle output `{}` in `{}` does not match return type `{}`",
                        parameter.name, function.name, function.returns
                    ));
                }
                rendered.push_str(&format!(
                    "{indent}    let mut raw: *mut ffi::{} = std::ptr::null_mut();\n",
                    output_record.c_name
                ));
                call_args.push("&mut raw".to_string());
                handle_output = Some(output_ty);
            }
            (Some(CParameterDirection::Output), _, false) if parameter.borrowed_array.is_some() => {
                let array = parameter.borrowed_array.as_ref().expect("matched array");
                let length_function = manifest
                    .modules
                    .iter()
                    .flat_map(|module| &module.functions)
                    .find(|candidate| candidate.c_symbol == array.length_symbol)
                    .ok_or_else(|| {
                        format!(
                            "error[native_bindgen.c_borrowed_array_contract]: no binding maps length symbol `{}`",
                            array.length_symbol
                        )
                    })?;
                if !inside_impl || length_function.returns != "Int" {
                    return Err(format!(
                        "error[native_bindgen.c_borrowed_array_contract]: `{}` requires an Int length method",
                        function.name
                    ));
                }
                let rust_ty = rust_ffi_type(base, aliases)?;
                let length_name = format!("{}_length", parameter.name);
                rendered.push_str(&format!(
                    "{indent}    let {length_name} = self.{}()?;\n{indent}    let mut out_{}: *mut {rust_ty} = std::ptr::null_mut();\n",
                    length_function.name, parameter.name
                ));
                call_args.push(format!("&mut out_{}", parameter.name));
                array_output = Some((format!("out_{}", parameter.name), length_name));
            }
            (Some(CParameterDirection::Output), _, false) => {
                let rust_ty = rust_ffi_type(base, aliases)?;
                rendered.push_str(&format!(
                    "{indent}    let mut out_{}: {rust_ty} = Default::default();\n",
                    parameter.name
                ));
                call_args.push(format!("&mut out_{}", parameter.name));
                scalar_output = Some((format!("out_{}", parameter.name), rust_ty));
            }
            (Some(CParameterDirection::Input), Some(CParameterOwnership::Value), false) => {
                let rust_ty = rust_ffi_type(base, aliases)?;
                call_args.push(format!("{} as {rust_ty}", parameter.name));
            }
            _ => {
                return Err(format!(
                    "error[native_bindgen.unsupported_wrapper_shape]: C parameter `{}` in `{}`",
                    parameter.name, function.name
                ));
            }
        }
    }

    let call = format!("ffi::{}({})", symbol.c_name, call_args.join(", "));
    if fallible {
        rendered.push_str(&format!(
            "{indent}    // SAFETY: generated arguments follow the reviewed ownership metadata.\n{indent}    let status = unsafe {{ {call} }};\n{indent}    check_status({:?}, status as i32, {})?;\n",
            function.operation,
            symbol.success_code.unwrap_or(0)
        ));
    } else {
        rendered.push_str(&format!(
            "{indent}    // SAFETY: generated arguments follow the reviewed ownership metadata.\n{indent}    let value = unsafe {{ {call} }};\n"
        ));
    }

    let result = if let Some(output_ty) = handle_output {
        let constructor = if output_ty.name == ty.name {
            "Self".to_string()
        } else {
            output_ty.name.clone()
        };
        format!(
            "let raw = NonNull::new(raw).{}(CAbiError {{ operation: {:?}, status: -1 }})?;\n{indent}    Ok({constructor} {{ raw }})",
            "ok_or",
            function.operation
        )
    } else if let Some((pointer, length, destructor)) = owned_string_output {
        if !fallible {
            return Err(format!(
                "error[native_bindgen.c_owned_string_contract]: `{}` must report status",
                function.name
            ));
        }
        format!(
            "let bytes = if {length} == 0 {{\n{indent}        Ok(Vec::new())\n{indent}    }} else {{\n{indent}        NonNull::new({pointer}).map(|pointer| {{\n{indent}            // SAFETY: the producer returned `{length}` initialized bytes; they are copied before destruction.\n{indent}            unsafe {{ std::slice::from_raw_parts(pointer.as_ptr().cast::<u8>(), {length}).to_vec() }}\n{indent}        }}).ok_or(CAbiError {{ operation: {:?}, status: -1 }})\n{indent}    }};\n{indent}    // SAFETY: ownership metadata transfers this pointer to the named destructor exactly once.\n{indent}    unsafe {{ ffi::{destructor}({pointer}) }};\n{indent}    let bytes = bytes?;\n{indent}    let value = String::from_utf8(bytes).map_err(|_| CAbiError {{ operation: {:?}, status: -3 }})?;\n{indent}    Ok(value)",
            function.operation, function.operation
        )
    } else if let Some((pointer, length, destructor, element)) = owned_array_output {
        if !fallible {
            return Err(format!(
                "error[native_bindgen.c_owned_array_contract]: `{}` must report status",
                function.name
            ));
        }
        let copied = format!(
            "let values = if {length} == 0 {{\n{indent}        Ok(Vec::new())\n{indent}    }} else {{\n{indent}        NonNull::new({pointer}).map(|pointer| {{\n{indent}            // SAFETY: the producer returned `{length}` initialized elements; they are copied before destruction.\n{indent}            unsafe {{ std::slice::from_raw_parts(pointer.as_ptr(), {length}).to_vec() }}\n{indent}        }}).ok_or(CAbiError {{ operation: {:?}, status: -1 }})\n{indent}    }};\n{indent}    // SAFETY: ownership metadata transfers this array to the named destructor exactly once.\n{indent}    unsafe {{ ffi::{destructor}({pointer}) }};\n{indent}    Ok(values?)",
            function.operation
        );
        if element == COwnedArrayElement::Bool8 {
            format!(
                "{}\n{indent}    let values = values?;\n{indent}    values.into_iter().map(|value| match value {{\n{indent}        0 => Ok(false),\n{indent}        1 => Ok(true),\n{indent}        _ => Err(CAbiError {{ operation: {:?}, status: -4 }}),\n{indent}    }}).collect()",
                copied.trim_end_matches(&format!("\n{indent}    Ok(values?)")),
                function.operation,
            )
        } else {
            copied
        }
    } else if let Some((values, lengths, count, destructor)) = owned_string_array_output {
        if !fallible {
            return Err(format!(
                "error[native_bindgen.c_owned_string_array_contract]: `{}` must report status",
                function.name
            ));
        }
        format!(
            "let bytes: Result<Vec<Vec<u8>>, CAbiError> = if {count} == 0 {{\n{indent}        Ok(Vec::new())\n{indent}    }} else {{\n{indent}        match (NonNull::new({values}), NonNull::new({lengths})) {{\n{indent}            (Some(values_pointer), Some(lengths_pointer)) => {{\n{indent}                // SAFETY: the producer returned `{count}` initialized pointers and lengths; every string is copied before destruction.\n{indent}                let value_pointers = unsafe {{ std::slice::from_raw_parts(values_pointer.as_ptr(), {count}) }};\n{indent}                let value_lengths = unsafe {{ std::slice::from_raw_parts(lengths_pointer.as_ptr(), {count}) }};\n{indent}                value_pointers.iter().zip(value_lengths).map(|(&value_pointer, &value_length)| {{\n{indent}                    if value_length == 0 {{\n{indent}                        Ok(Vec::new())\n{indent}                    }} else {{\n{indent}                        let value_pointer = NonNull::new(value_pointer).ok_or(CAbiError {{ operation: {:?}, status: -1 }})?;\n{indent}                        // SAFETY: the matching length describes this initialized string allocation.\n{indent}                        Ok(unsafe {{ std::slice::from_raw_parts(value_pointer.as_ptr().cast::<u8>(), value_length).to_vec() }})\n{indent}                    }}\n{indent}                }}).collect()\n{indent}            }}\n{indent}            _ => Err(CAbiError {{ operation: {:?}, status: -1 }}),\n{indent}        }}\n{indent}    }};\n{indent}    // SAFETY: ownership metadata transfers both arrays and their string elements to the named destructor exactly once.\n{indent}    unsafe {{ ffi::{destructor}({values}, {lengths}, {count}) }};\n{indent}    let bytes = bytes?;\n{indent}    bytes.into_iter().map(|value| String::from_utf8(value).map_err(|_| CAbiError {{ operation: {:?}, status: -3 }})).collect()",
            function.operation, function.operation, function.operation
        )
    } else if let Some((pointer, length)) = array_output {
        if !fallible {
            return Err(format!(
                "error[native_bindgen.c_borrowed_array_contract]: `{}` must report status",
                function.name
            ));
        }
        format!(
            "let length = usize::try_from({length}).map_err(|_| CAbiError {{ operation: {:?}, status: -2 }})?;\n{indent}    let values = if length == 0 {{\n{indent}        Vec::new()\n{indent}    }} else {{\n{indent}        let pointer = NonNull::new({pointer}).ok_or(CAbiError {{ operation: {:?}, status: -1 }})?;\n{indent}        // SAFETY: metadata ties this borrowed array to `self`; it is copied before the borrow ends.\n{indent}        unsafe {{ std::slice::from_raw_parts(pointer.as_ptr(), length).to_vec() }}\n{indent}    }};\n{indent}    Ok(values)",
            function.operation, function.operation
        )
    } else if let Some((value, _)) = scalar_output {
        if fallible {
            if function.returns == "Float" {
                format!("Ok({value} as f64)")
            } else if function.returns == "Bool" {
                format!("Ok({value})")
            } else {
                format!("Ok({value} as i64)")
            }
        } else {
            if function.returns == "Float" {
                format!("{value} as f64")
            } else if function.returns == "Bool" {
                value
            } else {
                format!("{value} as i64")
            }
        }
    } else if function.returns == "Unit" {
        if fallible { "Ok(())" } else { "()" }.to_string()
    } else if !fallible && function.returns == "Int" {
        "value as i64".to_string()
    } else {
        return Err(format!(
            "error[native_bindgen.unsupported_wrapper_shape]: no result mapping for `{}`",
            function.name
        ));
    };
    rendered.push_str(&format!("{indent}    {result}\n{indent}}}\n\n"));
    Ok(rendered)
}

fn render_dispatcher_wrapper_body(
    function: &CAbiBindingFunction,
    dispatcher: &CDispatcherBinding,
    call_symbol: &CSymbol,
    symbols: &BTreeMap<&str, &CSymbol>,
    record: &CSymbol,
    indent: &str,
) -> Result<String, String> {
    let duplicate = symbols
        .get(dispatcher.duplicate_handle_symbol.as_str())
        .copied()
        .ok_or_else(|| {
            format!(
                "error[native_bindgen.c_dispatcher_contract]: unknown duplicate symbol for `{}`",
                function.name
            )
        })?;
    let abi_version = parse_dispatcher_abi_version(&dispatcher.extension_abi_version)
        .map_err(|detail| format!("error[native_bindgen.c_dispatcher_contract]: {detail}"))?;
    let operation = format!("{}\0", dispatcher.operator_name);
    let overload = format!("{}\0", dispatcher.overload_name);
    let handle_arguments = function
        .args
        .iter()
        .filter(|argument| argument.ty == function.args[0].ty)
        .collect::<Vec<_>>();
    let mut body = String::new();
    for (index, argument) in handle_arguments.iter().enumerate() {
        let raw = format!("dispatcher_raw_{}", argument.name);
        let guard = format!("dispatcher_input_{}", argument.name);
        let source = if index == 0 {
            "self.raw.as_ptr()".to_string()
        } else {
            format!("{}.raw.as_ptr()", argument.name)
        };
        let duplicate_operation =
            format!("{}.duplicate_handle.{}", function.operation, argument.name);
        body.push_str(&format!(
            "{indent}    let mut {raw}: *mut ffi::{} = std::ptr::null_mut();\n{indent}    // SAFETY: the source handle is borrowed and the returned handle is an independent owner.\n{indent}    let status = unsafe {{ ffi::{}({source}, &mut {raw}) }};\n{indent}    check_status({duplicate_operation:?}, status as i32, {})?;\n{indent}    let {guard} = DispatcherInputGuard::new({raw}).ok_or(CAbiError {{ operation: {:?}, status: -1 }})?;\n",
            record.c_name,
            duplicate.c_name,
            duplicate.success_code.unwrap_or(0),
            function.operation,
        ));
    }
    for value in &dispatcher.stack {
        let argument = match value {
            CDispatcherStackValue::OwnedOptionalIntArgument { argument }
            | CDispatcherStackValue::OwnedOptionalHandleCopy { argument } => argument,
            _ => continue,
        };
        let allocator = symbols
            .get(
                dispatcher
                    .optional_value_allocator_symbol
                    .as_deref()
                    .expect("validated optional allocator"),
            )
            .copied()
            .expect("validated optional allocator symbol");
        let destructor = symbols
            .get(
                dispatcher
                    .optional_value_destructor_symbol
                    .as_deref()
                    .expect("validated optional destructor"),
            )
            .copied()
            .expect("validated optional destructor symbol");
        let raw = format!("dispatcher_optional_{argument}_raw");
        let guard = format!("dispatcher_optional_{argument}");
        let allocate_operation = format!("{}.optional.{argument}.allocate", function.operation);
        body.push_str(&format!(
            "{indent}    let mut {raw}: *mut u64 = std::ptr::null_mut();\n{indent}    // SAFETY: the reviewed allocator returns exclusive storage for one StableIValue.\n{indent}    let status = unsafe {{ ffi::{}(&mut {raw}) }};\n{indent}    check_status({allocate_operation:?}, status as i32, {})?;\n{indent}    let mut {guard} = DispatcherOptionalValueGuard::new({raw}, ffi::{}, {}).ok_or(CAbiError {{ operation: {:?}, status: -1 }})?;\n",
            allocator.c_name,
            allocator.success_code.unwrap_or(0),
            destructor.c_name,
            destructor.success_code.unwrap_or(0),
            function.operation,
        ));
    }
    for value in &dispatcher.stack {
        let CDispatcherStackValue::OwnedIntListArgument { argument } = value else {
            continue;
        };
        let allocator = symbols
            .get(
                dispatcher
                    .list_allocator_symbol
                    .as_deref()
                    .expect("validated list allocator"),
            )
            .copied()
            .expect("validated list allocator symbol");
        let push = symbols
            .get(
                dispatcher
                    .list_push_symbol
                    .as_deref()
                    .expect("validated list push"),
            )
            .copied()
            .expect("validated list push symbol");
        let destructor = symbols
            .get(
                dispatcher
                    .list_destructor_symbol
                    .as_deref()
                    .expect("validated list destructor"),
            )
            .copied()
            .expect("validated list destructor symbol");
        let raw = format!("dispatcher_list_{argument}_raw");
        let guard = format!("dispatcher_list_{argument}");
        let allocate_operation = format!("{}.list.{argument}.allocate", function.operation);
        let push_operation = format!("{}.list.{argument}.push", function.operation);
        body.push_str(&format!(
            "{indent}    let mut {raw}: *mut () = std::ptr::null_mut();\n{indent}    // SAFETY: the reviewed allocator returns one exclusive dispatcher list.\n{indent}    let status = unsafe {{ ffi::{}({argument}.len(), &mut {raw}) }};\n{indent}    check_status({allocate_operation:?}, status as i32, {})?;\n{indent}    let {guard} = DispatcherListGuard::new({raw}, ffi::{}, {}).ok_or(CAbiError {{ operation: {:?}, status: -1 }})?;\n{indent}    for element in {argument} {{\n{indent}        // SAFETY: the armed list guard owns the destination and integer StableIValues are copied by value.\n{indent}        let status = unsafe {{ ffi::{}({guard}.as_ptr(), *element as u64) }};\n{indent}        check_status({push_operation:?}, status as i32, {})?;\n{indent}    }}\n",
            allocator.c_name,
            allocator.success_code.unwrap_or(0),
            destructor.c_name,
            destructor.success_code.unwrap_or(0),
            function.operation,
            push.c_name,
            push.success_code.unwrap_or(0),
        ));
    }
    for (index, value) in dispatcher.stack.iter().enumerate() {
        let CDispatcherStackValue::OwnedStringLiteral { value } = value else {
            continue;
        };
        let allocator = symbols
            .get(
                dispatcher
                    .string_allocator_symbol
                    .as_deref()
                    .expect("validated string allocator"),
            )
            .copied()
            .expect("validated string allocator symbol");
        let destructor = symbols
            .get(
                dispatcher
                    .string_destructor_symbol
                    .as_deref()
                    .expect("validated string destructor"),
            )
            .copied()
            .expect("validated string destructor symbol");
        let raw = format!("dispatcher_string_{index}_raw");
        let guard = format!("dispatcher_string_{index}");
        let bytes = format!("dispatcher_string_{index}_bytes");
        let allocate_operation = format!("{}.string.{index}.allocate", function.operation);
        body.push_str(&format!(
            "{indent}    let {bytes}: &[u8] = {value:?}.as_bytes();\n{indent}    let mut {raw}: *mut () = std::ptr::null_mut();\n{indent}    // SAFETY: the reviewed allocator copies the fixed metadata bytes into one exclusive dispatcher string.\n{indent}    let status = unsafe {{ ffi::{}({bytes}.as_ptr().cast(), {bytes}.len(), &mut {raw}) }};\n{indent}    check_status({allocate_operation:?}, status as i32, {})?;\n{indent}    let {guard} = DispatcherStringGuard::new({raw}, ffi::{}, {}).ok_or(CAbiError {{ operation: {:?}, status: -1 }})?;\n",
            allocator.c_name,
            allocator.success_code.unwrap_or(0),
            destructor.c_name,
            destructor.success_code.unwrap_or(0),
            function.operation,
        ));
    }
    for value in &dispatcher.stack {
        match value {
            CDispatcherStackValue::OwnedOptionalIntArgument { argument } => {
                body.push_str(&format!(
                    "{indent}    dispatcher_optional_{argument}.write_i64({argument});\n"
                ));
            }
            CDispatcherStackValue::OwnedOptionalHandleCopy { argument } => {
                body.push_str(&format!(
                    "{indent}    dispatcher_optional_{argument}.write_stable_ivalue(dispatcher_input_{argument}.into_stable_ivalue());\n"
                ));
            }
            _ => {}
        }
    }
    let stack = dispatcher
        .stack
        .iter()
        .enumerate()
        .map(|(index, value)| match value {
            CDispatcherStackValue::OwnedHandleCopy { argument } => {
                format!("dispatcher_input_{argument}.into_stable_ivalue()")
            }
            CDispatcherStackValue::OwnedOptionalHandleCopy { argument } => {
                format!("dispatcher_optional_{argument}.into_stable_ivalue()")
            }
            CDispatcherStackValue::OwnedIntListArgument { argument } => {
                format!("dispatcher_list_{argument}.into_stable_ivalue()")
            }
            CDispatcherStackValue::OwnedStringLiteral { .. } => {
                format!("dispatcher_string_{index}.into_stable_ivalue()")
            }
            CDispatcherStackValue::IntArgument { argument } => {
                format!("{argument} as u64")
            }
            CDispatcherStackValue::FloatArgument { argument } => {
                format!("{argument}.to_bits()")
            }
            CDispatcherStackValue::BoolArgument { argument } => {
                format!("u64::from({argument})")
            }
            CDispatcherStackValue::OwnedOptionalIntArgument { argument } => {
                format!("dispatcher_optional_{argument}.into_stable_ivalue()")
            }
            CDispatcherStackValue::Null => "0u64".to_string(),
            CDispatcherStackValue::Unsupported => unreachable!("validated dispatcher stack"),
        })
        .collect::<Vec<_>>()
        .join(", ");
    body.push_str(&format!(
        "{indent}    // StableIValue takes ownership of copied handle inputs; the selected output slot transfers ownership back.\n{indent}    let mut stack = [{stack}];\n{indent}    // SAFETY: metadata fixes the operator schema, stack layout, ABI version, and ownership transfer.\n{indent}    let status = unsafe {{ ffi::{}({:?}.as_ptr().cast(), {:?}.as_ptr().cast(), stack.as_mut_ptr(), 0x{abi_version:016x}u64) }};\n{indent}    check_status({:?}, status as i32, {})?;\n{indent}    let raw = stack[{}] as usize as *mut ffi::{};\n{indent}    let raw = NonNull::new(raw).ok_or(CAbiError {{ operation: {:?}, status: -1 }})?;\n{indent}    Ok(Self {{ raw }})\n",
        call_symbol.c_name,
        operation,
        overload,
        function.operation,
        call_symbol.success_code.unwrap_or(0),
        dispatcher.output.index,
        record.c_name,
        function.operation,
    ));
    Ok(body)
}

fn render_raw_ffi_function(
    symbol: &CSymbol,
    aliases: &BTreeMap<String, String>,
) -> Result<String, String> {
    let args = symbol
        .parameters
        .iter()
        .map(|parameter| {
            Ok(format!(
                "{}: {}",
                parameter.name,
                rust_ffi_type(&parameter.c_type, aliases)?
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let returns = rust_ffi_type(symbol.returns.as_deref().unwrap_or("void"), aliases)?;
    let return_text = if returns == "()" {
        String::new()
    } else {
        format!(" -> {returns}")
    };
    Ok(format!(
        "        pub fn {}({}){};\n",
        symbol.c_name,
        args.join(", "),
        return_text
    ))
}

fn rust_ffi_type(c_type: &str, aliases: &BTreeMap<String, String>) -> Result<String, String> {
    let resolved = resolve_c_type(c_type, aliases)?;
    let c_type = resolved.trim();
    let pointer_depth = c_type.matches('*').count();
    let is_const = c_type.starts_with("const ");
    let base = c_pointer_base(c_type);
    let mut rust_type = match base {
        "void" => "()".to_string(),
        "bool" => "bool".to_string(),
        "int8_t" => "i8".to_string(),
        "uint8_t" => "u8".to_string(),
        "int16_t" => "i16".to_string(),
        "uint16_t" => "u16".to_string(),
        "int32_t" => "i32".to_string(),
        "uint32_t" => "u32".to_string(),
        "int64_t" => "i64".to_string(),
        "uint64_t" => "u64".to_string(),
        "size_t" => "usize".to_string(),
        "float" => "f32".to_string(),
        "double" => "f64".to_string(),
        "char" => "std::ffi::c_char".to_string(),
        value if is_c_identifier(value) => value.to_string(),
        _ => {
            return Err(format!(
                "error[native_bindgen.unsupported_c_type]: `{c_type}` has no Rust FFI mapping"
            ));
        }
    };
    for depth in 0..pointer_depth {
        let qualifier = if depth == 0 && is_const {
            "*const"
        } else {
            "*mut"
        };
        rust_type = format!("{qualifier} {rust_type}");
    }
    Ok(rust_type)
}
