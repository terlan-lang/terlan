
fn validate_c_type_references(
    symbols: &BTreeMap<&str, &CSymbol>,
    aliases: &BTreeMap<String, String>,
) -> Result<(), String> {
    let opaque_names = symbols
        .values()
        .filter(|symbol| {
            symbol.status == CSymbolStatus::Bind && symbol.kind == CSymbolKind::OpaqueStruct
        })
        .map(|symbol| symbol.c_name.as_str())
        .collect::<BTreeSet<_>>();
    for symbol in symbols.values().filter(|symbol| {
        symbol.status == CSymbolStatus::Bind && symbol.kind == CSymbolKind::Function
    }) {
        for parameter in &symbol.parameters {
            let resolved = resolve_c_type(&parameter.c_type, aliases)?;
            let base = c_pointer_base(&resolved);
            if !is_builtin_c_type(base) && !opaque_names.contains(base) {
                return Err(format!(
                    "error[native_bindgen.c_type_unmapped]: C symbol `{}` references `{base}`",
                    symbol.id
                ));
            }
        }
        if let Some(returns) = symbol.returns.as_deref() {
            let resolved = resolve_c_type(returns, aliases)?;
            let base = c_pointer_base(&resolved);
            if !is_builtin_c_type(base) {
                return Err(format!(
                    "error[native_bindgen.c_type_unmapped]: C symbol `{}` returns `{base}` by value",
                    symbol.id
                ));
            }
        }
    }
    Ok(())
}

fn validate_borrowed_arrays(
    symbols: &BTreeMap<&str, &CSymbol>,
    aliases: &BTreeMap<String, String>,
) -> Result<(), String> {
    let opaque_names = symbols
        .values()
        .filter(|symbol| {
            symbol.status == CSymbolStatus::Bind && symbol.kind == CSymbolKind::OpaqueStruct
        })
        .map(|symbol| symbol.c_name.as_str())
        .collect::<BTreeSet<_>>();
    for symbol in symbols.values().filter(|symbol| {
        symbol.status == CSymbolStatus::Bind && symbol.kind == CSymbolKind::Function
    }) {
        for parameter in &symbol.parameters {
            let Some(array) = &parameter.borrowed_array else {
                continue;
            };
            let resolved = resolve_c_type(&parameter.c_type, aliases)?;
            if parameter.direction != Some(CParameterDirection::Output)
                || parameter.ownership != Some(CParameterOwnership::BorrowedCall)
                || resolved.matches('*').count() != 2
                || c_pointer_base(&resolved) != "int64_t"
                || array.copy != CBorrowedArrayCopy::Immediate
            {
                return Err(format!(
                    "error[native_bindgen.c_borrowed_array_contract]: `{}` must be an immediately copied int64_t output pointer",
                    parameter.name
                ));
            }
            let owner = symbol
                .parameters
                .iter()
                .find(|candidate| candidate.name == array.owner_parameter)
                .ok_or_else(|| {
                    format!(
                        "error[native_bindgen.c_borrowed_array_contract]: `{}` names unknown owner parameter `{}`",
                        parameter.name, array.owner_parameter
                    )
                })?;
            let owner_resolved = resolve_c_type(&owner.c_type, aliases)?;
            if owner.direction != Some(CParameterDirection::Input)
                || owner.ownership != Some(CParameterOwnership::BorrowedCall)
                || !opaque_names.contains(c_pointer_base(&owner_resolved))
            {
                return Err(format!(
                    "error[native_bindgen.c_borrowed_array_contract]: `{}` requires a borrowed opaque owner",
                    parameter.name
                ));
            }
            let length = symbols.get(array.length_symbol.as_str()).ok_or_else(|| {
                format!(
                    "error[native_bindgen.c_borrowed_array_contract]: `{}` names unknown length symbol `{}`",
                    parameter.name, array.length_symbol
                )
            })?;
            let length_outputs = length
                .parameters
                .iter()
                .filter(|candidate| candidate.direction == Some(CParameterDirection::Output))
                .collect::<Vec<_>>();
            if length.status != CSymbolStatus::Bind
                || length.kind != CSymbolKind::Function
                || length.error_model != Some(CErrorModel::StatusCode)
                || length_outputs.len() != 1
                || resolve_c_type(&length_outputs[0].c_type, aliases)? != "int64_t *"
            {
                return Err(format!(
                    "error[native_bindgen.c_borrowed_array_contract]: length symbol `{}` must return one int64_t output under status control",
                    array.length_symbol
                ));
            }
        }
    }
    Ok(())
}

fn validate_owned_strings(
    symbols: &BTreeMap<&str, &CSymbol>,
    aliases: &BTreeMap<String, String>,
) -> Result<(), String> {
    for symbol in symbols.values().filter(|symbol| {
        symbol.status == CSymbolStatus::Bind && symbol.kind == CSymbolKind::Function
    }) {
        for parameter in &symbol.parameters {
            let Some(string) = &parameter.owned_string else {
                continue;
            };
            let resolved = resolve_c_type(&parameter.c_type, aliases)?;
            if symbol.error_model != Some(CErrorModel::StatusCode)
                || parameter.direction != Some(CParameterDirection::Output)
                || parameter.ownership != Some(CParameterOwnership::TransferFull)
                || resolved != "char **"
                || string.copy != COwnedStringCopy::ImmediateUtf8
                || parameter.borrowed_array.is_some()
                || parameter.input_array.is_some()
                || parameter.owned_array.is_some()
                || parameter.fixed.is_some()
            {
                return Err(format!(
                    "error[native_bindgen.c_owned_string_contract]: `{}` must be a status-controlled, immediately copied char ** output",
                    parameter.name
                ));
            }
            let length = symbol
                .parameters
                .iter()
                .find(|candidate| candidate.name == string.length_parameter)
                .ok_or_else(|| {
                    format!(
                        "error[native_bindgen.c_owned_string_contract]: `{}` names unknown length parameter `{}`",
                        parameter.name, string.length_parameter
                    )
                })?;
            if length.name == parameter.name
                || length.direction != Some(CParameterDirection::Output)
                || length.ownership != Some(CParameterOwnership::BorrowedCall)
                || resolve_c_type(&length.c_type, aliases)? != "size_t *"
                || length.borrowed_array.is_some()
                || length.input_array.is_some()
                || length.owned_string.is_some()
                || length.owned_array.is_some()
                || length.fixed.is_some()
            {
                return Err(format!(
                    "error[native_bindgen.c_owned_string_contract]: `{}` requires a distinct size_t output length parameter",
                    parameter.name
                ));
            }
            let owners = symbol
                .parameters
                .iter()
                .filter(|candidate| {
                    candidate.owned_string.as_ref().is_some_and(|candidate| {
                        candidate.length_parameter == string.length_parameter
                    })
                })
                .count();
            if owners != 1 {
                return Err(format!(
                    "error[native_bindgen.c_owned_string_contract]: length parameter `{}` must belong to exactly one owned string",
                    string.length_parameter
                ));
            }
            let destructor = symbols
                .get(string.destructor_symbol.as_str())
                .ok_or_else(|| {
                    format!(
                        "error[native_bindgen.c_owned_string_contract]: `{}` names unknown destructor `{}`",
                        parameter.name, string.destructor_symbol
                    )
                })?;
            let valid_destructor = destructor.status == CSymbolStatus::Bind
                && destructor.kind == CSymbolKind::Function
                && destructor.returns.as_deref() == Some("void")
                && destructor.error_model == Some(CErrorModel::Infallible)
                && destructor.parameters.len() == 1;
            if !valid_destructor {
                return Err(format!(
                    "error[native_bindgen.c_owned_string_contract]: destructor `{}` must be an infallible void function with one parameter",
                    string.destructor_symbol
                ));
            }
            let destructor_parameter = &destructor.parameters[0];
            if destructor_parameter.direction != Some(CParameterDirection::Input)
                || destructor_parameter.ownership != Some(CParameterOwnership::TransferFull)
                || resolve_c_type(&destructor_parameter.c_type, aliases)? != "char *"
            {
                return Err(format!(
                    "error[native_bindgen.c_owned_string_contract]: destructor `{}` must consume one owned char pointer",
                    string.destructor_symbol
                ));
            }
        }
    }
    Ok(())
}

fn validate_owned_arrays(
    symbols: &BTreeMap<&str, &CSymbol>,
    aliases: &BTreeMap<String, String>,
) -> Result<(), String> {
    for symbol in symbols.values().filter(|symbol| {
        symbol.status == CSymbolStatus::Bind && symbol.kind == CSymbolKind::Function
    }) {
        for parameter in &symbol.parameters {
            let Some(array) = &parameter.owned_array else {
                continue;
            };
            let resolved = resolve_c_type(&parameter.c_type, aliases)?;
            let expected_output = array.element.c_output_pointer();
            if symbol.error_model != Some(CErrorModel::StatusCode)
                || parameter.direction != Some(CParameterDirection::Output)
                || parameter.ownership != Some(CParameterOwnership::TransferFull)
                || resolved != expected_output
                || array.copy != COwnedArrayCopy::Immediate
                || parameter.borrowed_array.is_some()
                || parameter.input_array.is_some()
                || parameter.owned_string.is_some()
                || parameter.fixed.is_some()
            {
                return Err(format!(
                    "error[native_bindgen.c_owned_array_contract]: `{}` must be a status-controlled, immediately copied {expected_output} output",
                    parameter.name,
                ));
            }
            let length = symbol
                .parameters
                .iter()
                .find(|candidate| candidate.name == array.length_parameter)
                .ok_or_else(|| {
                    format!(
                        "error[native_bindgen.c_owned_array_contract]: `{}` names unknown length parameter `{}`",
                        parameter.name, array.length_parameter
                    )
                })?;
            if length.name == parameter.name
                || length.direction != Some(CParameterDirection::Output)
                || length.ownership != Some(CParameterOwnership::BorrowedCall)
                || resolve_c_type(&length.c_type, aliases)? != "size_t *"
                || length.borrowed_array.is_some()
                || length.input_array.is_some()
                || length.owned_string.is_some()
                || length.owned_array.is_some()
                || length.fixed.is_some()
            {
                return Err(format!(
                    "error[native_bindgen.c_owned_array_contract]: `{}` requires a distinct size_t output length parameter",
                    parameter.name
                ));
            }
            let owners = symbol
                .parameters
                .iter()
                .filter(|candidate| {
                    candidate.owned_array.as_ref().is_some_and(|candidate| {
                        candidate.length_parameter == array.length_parameter
                    })
                })
                .count();
            if owners != 1 {
                return Err(format!(
                    "error[native_bindgen.c_owned_array_contract]: length parameter `{}` must belong to exactly one owned array",
                    array.length_parameter
                ));
            }
            let destructor = symbols
                .get(array.destructor_symbol.as_str())
                .ok_or_else(|| {
                    format!(
                        "error[native_bindgen.c_owned_array_contract]: `{}` names unknown destructor `{}`",
                        parameter.name, array.destructor_symbol
                    )
                })?;
            let valid_destructor = destructor.status == CSymbolStatus::Bind
                && destructor.kind == CSymbolKind::Function
                && destructor.returns.as_deref() == Some("void")
                && destructor.error_model == Some(CErrorModel::Infallible)
                && destructor.parameters.len() == 1;
            if !valid_destructor {
                return Err(format!(
                    "error[native_bindgen.c_owned_array_contract]: destructor `{}` must be an infallible void function with one parameter",
                    array.destructor_symbol
                ));
            }
            let destructor_parameter = &destructor.parameters[0];
            if destructor_parameter.direction != Some(CParameterDirection::Input)
                || destructor_parameter.ownership != Some(CParameterOwnership::TransferFull)
                || resolve_c_type(&destructor_parameter.c_type, aliases)?
                    != array.element.c_pointer()
            {
                return Err(format!(
                    "error[native_bindgen.c_owned_array_contract]: destructor `{}` must consume one owned {} pointer",
                    array.destructor_symbol,
                    array.element.c_pointer(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_owned_string_arrays(
    symbols: &BTreeMap<&str, &CSymbol>,
    aliases: &BTreeMap<String, String>,
) -> Result<(), String> {
    for symbol in symbols.values().filter(|symbol| {
        symbol.status == CSymbolStatus::Bind && symbol.kind == CSymbolKind::Function
    }) {
        for parameter in &symbol.parameters {
            let Some(array) = &parameter.owned_string_array else {
                continue;
            };
            if symbol.error_model != Some(CErrorModel::StatusCode)
                || parameter.direction != Some(CParameterDirection::Output)
                || parameter.ownership != Some(CParameterOwnership::TransferFull)
                || resolve_c_type(&parameter.c_type, aliases)? != "char ***"
                || array.copy != COwnedStringCopy::ImmediateUtf8
                || parameter.borrowed_array.is_some()
                || parameter.input_array.is_some()
                || parameter.owned_string.is_some()
                || parameter.owned_array.is_some()
                || parameter.fixed.is_some()
            {
                return Err(format!(
                    "error[native_bindgen.c_owned_string_array_contract]: `{}` must be a status-controlled, immediately copied char *** output",
                    parameter.name
                ));
            }
            let lengths = symbol
                .parameters
                .iter()
                .find(|candidate| candidate.name == array.lengths_parameter)
                .ok_or_else(|| format!(
                    "error[native_bindgen.c_owned_string_array_contract]: `{}` names unknown lengths parameter `{}`",
                    parameter.name, array.lengths_parameter
                ))?;
            let count = symbol
                .parameters
                .iter()
                .find(|candidate| candidate.name == array.count_parameter)
                .ok_or_else(|| format!(
                    "error[native_bindgen.c_owned_string_array_contract]: `{}` names unknown count parameter `{}`",
                    parameter.name, array.count_parameter
                ))?;
            if parameter.name == lengths.name
                || parameter.name == count.name
                || lengths.name == count.name
                || lengths.direction != Some(CParameterDirection::Output)
                || lengths.ownership != Some(CParameterOwnership::TransferFull)
                || resolve_c_type(&lengths.c_type, aliases)? != "size_t **"
                || count.direction != Some(CParameterDirection::Output)
                || count.ownership != Some(CParameterOwnership::BorrowedCall)
                || resolve_c_type(&count.c_type, aliases)? != "size_t *"
                || parameter_metadata_count(lengths) != 0
                || parameter_metadata_count(count) != 0
            {
                return Err(format!(
                    "error[native_bindgen.c_owned_string_array_contract]: `{}` requires distinct owned size_t ** lengths and borrowed size_t * count outputs",
                    parameter.name
                ));
            }
            let owners = symbol
                .parameters
                .iter()
                .filter(|candidate| {
                    candidate
                        .owned_string_array
                        .as_ref()
                        .is_some_and(|candidate| {
                            candidate.lengths_parameter == array.lengths_parameter
                                || candidate.count_parameter == array.count_parameter
                        })
                })
                .count();
            if owners != 1 {
                return Err(format!(
                    "error[native_bindgen.c_owned_string_array_contract]: lengths `{}` and count `{}` must belong to exactly one owned string array",
                    array.lengths_parameter, array.count_parameter
                ));
            }
            let destructor = symbols.get(array.destructor_symbol.as_str()).ok_or_else(|| {
                format!(
                    "error[native_bindgen.c_owned_string_array_contract]: `{}` names unknown destructor `{}`",
                    parameter.name, array.destructor_symbol
                )
            })?;
            let valid_destructor = destructor.status == CSymbolStatus::Bind
                && destructor.kind == CSymbolKind::Function
                && destructor.returns.as_deref() == Some("void")
                && destructor.error_model == Some(CErrorModel::Infallible)
                && destructor.parameters.len() == 3;
            if !valid_destructor {
                return Err(format!(
                    "error[native_bindgen.c_owned_string_array_contract]: destructor `{}` must be an infallible void function with values, lengths, and count parameters",
                    array.destructor_symbol
                ));
            }
            let values = &destructor.parameters[0];
            let destructor_lengths = &destructor.parameters[1];
            let destructor_count = &destructor.parameters[2];
            if values.direction != Some(CParameterDirection::Input)
                || values.ownership != Some(CParameterOwnership::TransferFull)
                || resolve_c_type(&values.c_type, aliases)? != "char **"
                || destructor_lengths.direction != Some(CParameterDirection::Input)
                || destructor_lengths.ownership != Some(CParameterOwnership::TransferFull)
                || resolve_c_type(&destructor_lengths.c_type, aliases)? != "size_t *"
                || destructor_count.direction != Some(CParameterDirection::Input)
                || destructor_count.ownership != Some(CParameterOwnership::Value)
                || resolve_c_type(&destructor_count.c_type, aliases)? != "size_t"
            {
                return Err(format!(
                    "error[native_bindgen.c_owned_string_array_contract]: destructor `{}` must consume owned char ** values, owned size_t * lengths, and a size_t count",
                    array.destructor_symbol
                ));
            }
        }
    }
    Ok(())
}

fn parameter_metadata_count(parameter: &CParameter) -> usize {
    usize::from(parameter.borrowed_array.is_some())
        + usize::from(parameter.input_array.is_some())
        + usize::from(parameter.owned_string.is_some())
        + usize::from(parameter.owned_array.is_some())
        + usize::from(parameter.owned_string_array.is_some())
        + usize::from(parameter.fixed.is_some())
}

fn validate_c_symbol(symbol: &CSymbol, aliases: &BTreeMap<String, String>) -> Result<(), String> {
    if symbol.id.trim().is_empty() || !is_c_identifier(&symbol.c_name) {
        return Err(format!(
            "structured C symbol `{}` requires a stable id and C identifier",
            symbol.id
        ));
    }
    match symbol.status {
        CSymbolStatus::Unsupported => {
            let shape = symbol.unsupported_shape.ok_or_else(|| {
                format!(
                    "unsupported C symbol `{}` requires unsupported_shape",
                    symbol.id
                )
            })?;
            if symbol.detail.as_deref().is_none_or(str::is_empty) {
                return Err(format!(
                    "unsupported C symbol `{}` requires stable detail",
                    symbol.id
                ));
            }
            let _ = skip_reason(shape);
        }
        CSymbolStatus::Bind => {
            if symbol.unsupported_shape.is_some() {
                return Err(format!(
                    "bindable C symbol `{}` cannot carry unsupported_shape",
                    symbol.id
                ));
            }
            match symbol.kind {
                CSymbolKind::OpaqueStruct => {
                    if symbol.ownership.as_deref() != Some("owned") {
                        return Err(stable_shape_error(
                            symbol,
                            UnsupportedCShape::PointerOwnershipUnknown,
                        ));
                    }
                    if symbol
                        .destructor_symbol
                        .as_deref()
                        .is_none_or(str::is_empty)
                    {
                        return Err(stable_shape_error(
                            symbol,
                            UnsupportedCShape::MissingDestructor,
                        ));
                    }
                    if !matches!(
                        symbol.thread_safety.as_deref(),
                        Some("thread_confined" | "send_only")
                    ) {
                        return Err(format!(
                            "opaque C symbol `{}` requires `thread_confined` or `send_only` thread-safety metadata",
                            symbol.id
                        ));
                    }
                }
                CSymbolKind::Function => {
                    if symbol.variadic {
                        return Err(stable_shape_error(
                            symbol,
                            UnsupportedCShape::UnsupportedVariadicFunction,
                        ));
                    }
                    if symbol.callback {
                        return Err(stable_shape_error(
                            symbol,
                            UnsupportedCShape::UnsupportedCallback,
                        ));
                    }
                    let returns = symbol.returns.as_deref().ok_or_else(|| {
                        format!("bindable C function `{}` requires returns", symbol.id)
                    })?;
                    validate_c_return_shape(symbol, returns, aliases)?;
                    match symbol.error_model {
                        Some(CErrorModel::StatusCode) if symbol.success_code.is_some() => {}
                        Some(CErrorModel::Infallible) if symbol.success_code.is_none() => {}
                        Some(CErrorModel::StatusCode) => {
                            return Err(format!(
                                "C status function `{}` requires success_code",
                                symbol.id
                            ));
                        }
                        Some(CErrorModel::Infallible) => {
                            return Err(format!(
                                "infallible C function `{}` cannot declare success_code",
                                symbol.id
                            ));
                        }
                        None => {
                            return Err(format!(
                                "bindable C function `{}` requires an error_model",
                                symbol.id
                            ));
                        }
                    }
                    for parameter in &symbol.parameters {
                        validate_lower_identifier("C parameter", &parameter.name)?;
                        validate_c_parameter_shape(symbol, parameter, aliases)?;
                    }
                    validate_c_input_arrays(symbol, aliases)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_c_return_shape(
    symbol: &CSymbol,
    c_type: &str,
    aliases: &BTreeMap<String, String>,
) -> Result<(), String> {
    let resolved = resolve_c_type(c_type, aliases)?;
    if resolved.contains('*') {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCShape::BorrowedLifetime,
        ));
    }
    validate_supported_c_scalar(&resolved)
}

fn validate_c_parameter_shape(
    symbol: &CSymbol,
    parameter: &CParameter,
    aliases: &BTreeMap<String, String>,
) -> Result<(), String> {
    let direction = parameter
        .direction
        .ok_or_else(|| stable_shape_error(symbol, UnsupportedCShape::PointerOwnershipUnknown))?;
    let ownership = parameter
        .ownership
        .ok_or_else(|| stable_shape_error(symbol, UnsupportedCShape::PointerOwnershipUnknown))?;
    let resolved = resolve_c_type(&parameter.c_type, aliases)?;
    if resolved.contains("(*") {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCShape::UnsupportedCallback,
        ));
    }
    let pointer_depth = resolved.matches('*').count();
    if let Some(fixed) = &parameter.fixed {
        validate_c_fixed_input(parameter, fixed, &resolved, direction, ownership)?;
    }
    if pointer_depth == 0 {
        if direction != CParameterDirection::Input || ownership != CParameterOwnership::Value {
            return Err(stable_shape_error(
                symbol,
                UnsupportedCShape::PointerOwnershipUnknown,
            ));
        }
        return validate_supported_c_scalar(&resolved);
    }
    if pointer_depth > 3
        || (pointer_depth == 3
            && (parameter.owned_string_array.is_none() || c_pointer_base(&resolved) != "char"))
        || ownership == CParameterOwnership::Value
    {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCShape::PointerOwnershipUnknown,
        ));
    }
    if pointer_depth == 2
        && ownership == CParameterOwnership::BorrowedCall
        && parameter.borrowed_array.is_none()
    {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCShape::BorrowedLifetime,
        ));
    }
    if parameter.borrowed_array.is_some()
        && (pointer_depth != 2
            || direction != CParameterDirection::Output
            || ownership != CParameterOwnership::BorrowedCall)
    {
        return Err(format!(
            "error[native_bindgen.c_borrowed_array_contract]: `{}` has an invalid borrowed array shape",
            parameter.name
        ));
    }
    if parameter.input_array.is_some()
        && (pointer_depth != 1
            || direction != CParameterDirection::Input
            || ownership != CParameterOwnership::BorrowedCall
            || !resolved.trim().starts_with("const ")
            || !matches!(c_pointer_base(&resolved), "int64_t" | "double" | "uint8_t"))
    {
        return Err(format!(
            "error[native_bindgen.c_input_array_contract]: `{}` must be a borrowed const int64_t, double, or uint8_t input pointer",
            parameter.name
        ));
    }
    if parameter.input_array.is_some()
        && (parameter.borrowed_array.is_some()
            || parameter.owned_string.is_some()
            || parameter.owned_array.is_some()
            || parameter.owned_string_array.is_some()
            || parameter.fixed.is_some())
    {
        return Err(format!(
            "error[native_bindgen.c_input_array_contract]: `{}` cannot combine input-array, output-array, or fixed-input metadata",
            parameter.name
        ));
    }
    if ownership == CParameterOwnership::TransferFull
        && !matches!(
            direction,
            CParameterDirection::Input | CParameterDirection::Output
        )
    {
        return Err(stable_shape_error(
            symbol,
            UnsupportedCShape::PointerOwnershipUnknown,
        ));
    }
    let base = c_pointer_base(&resolved);
    validate_supported_c_scalar(base)
}

fn validate_c_fixed_input(
    parameter: &CParameter,
    fixed: &CFixedInput,
    resolved: &str,
    direction: CParameterDirection,
    ownership: CParameterOwnership,
) -> Result<(), String> {
    let pointer_depth = resolved.matches('*').count();
    let base = c_pointer_base(resolved);
    let valid = match fixed {
        CFixedInput::Null => {
            pointer_depth == 1
                && direction == CParameterDirection::Input
                && ownership == CParameterOwnership::BorrowedCall
        }
        CFixedInput::Int32 { .. } => {
            base == "int32_t"
                && direction == CParameterDirection::Input
                && ((pointer_depth == 0 && ownership == CParameterOwnership::Value)
                    || (pointer_depth == 1 && ownership == CParameterOwnership::BorrowedCall))
        }
    };
    if !valid || parameter.input_array.is_some() || parameter.borrowed_array.is_some() {
        return Err(format!(
            "error[native_bindgen.c_fixed_input_contract]: `{}` has an invalid fixed input shape",
            parameter.name
        ));
    }
    Ok(())
}

fn validate_c_input_arrays(
    symbol: &CSymbol,
    aliases: &BTreeMap<String, String>,
) -> Result<(), String> {
    for parameter in &symbol.parameters {
        let Some(array) = &parameter.input_array else {
            continue;
        };
        let length = symbol
            .parameters
            .iter()
            .find(|candidate| candidate.name == array.length_parameter)
            .ok_or_else(|| {
                format!(
                    "error[native_bindgen.c_input_array_contract]: `{}` references missing length parameter `{}`",
                    parameter.name, array.length_parameter
                )
            })?;
        if array.length_parameter == parameter.name
            || length.direction != Some(CParameterDirection::Input)
            || length.ownership != Some(CParameterOwnership::Value)
            || resolve_c_type(&length.c_type, aliases)? != "int64_t"
            || length.input_array.is_some()
            || length.borrowed_array.is_some()
            || length.fixed.is_some()
        {
            return Err(format!(
                "error[native_bindgen.c_input_array_contract]: `{}` requires a distinct int64_t value length parameter",
                parameter.name
            ));
        }
        let owners = symbol
            .parameters
            .iter()
            .filter(|candidate| {
                candidate
                    .input_array
                    .as_ref()
                    .is_some_and(|candidate| candidate.length_parameter == array.length_parameter)
            })
            .count();
        if owners != 1 {
            return Err(format!(
                "error[native_bindgen.c_input_array_contract]: length parameter `{}` must belong to exactly one input array",
                array.length_parameter
            ));
        }
    }
    Ok(())
}

fn validate_input_array_binding(
    function: &CAbiBindingFunction,
    symbol: &CSymbol,
    aliases: &BTreeMap<String, String>,
) -> Result<(), String> {
    for parameter in symbol
        .parameters
        .iter()
        .filter(|parameter| parameter.input_array.is_some())
    {
        let argument = function
            .args
            .iter()
            .find(|argument| argument.name == parameter.name)
            .ok_or_else(|| {
                format!(
                    "error[native_bindgen.c_input_array_contract]: `{}` has no matching Terlan argument in `{}`",
                    parameter.name, function.name
                )
            })?;
        let resolved = resolve_c_type(&parameter.c_type, aliases)?;
        let expected = match c_pointer_base(&resolved) {
            "int64_t" => "List[Int]",
            "double" => "List[Float]",
            "uint8_t" => "List[Bool]",
            base => {
                return Err(format!(
                    "error[native_bindgen.c_input_array_contract]: `{}` uses unsupported input element type `{base}`",
                    parameter.name
                ));
            }
        };
        if argument.ty != expected {
            return Err(format!(
                "error[native_bindgen.c_input_array_contract]: `{}` requires `{expected}` for `{}`",
                parameter.name, function.name
            ));
        }
    }
    Ok(())
}

fn validate_owned_string_binding(
    function: &CAbiBindingFunction,
    symbol: &CSymbol,
) -> Result<(), String> {
    let outputs = symbol
        .parameters
        .iter()
        .filter(|parameter| parameter.owned_string.is_some())
        .count();
    if (function.returns == "String" && outputs != 1)
        || (function.returns != "String" && outputs != 0)
    {
        return Err(format!(
            "error[native_bindgen.c_owned_string_contract]: `{}` must map one owned string output to a String return",
            function.name
        ));
    }
    Ok(())
}

fn validate_owned_array_binding(
    function: &CAbiBindingFunction,
    symbol: &CSymbol,
) -> Result<(), String> {
    let outputs = symbol
        .parameters
        .iter()
        .filter_map(|parameter| parameter.owned_array.as_ref())
        .collect::<Vec<_>>();
    let expected = outputs.first().map(|array| array.element.terlan_list());
    if outputs.len() > 1 || expected.is_some_and(|expected| function.returns != expected) {
        return Err(format!(
            "error[native_bindgen.c_owned_array_contract]: `{}` must map one owned array output to its declared element-list return",
            function.name,
        ));
    }
    Ok(())
}
