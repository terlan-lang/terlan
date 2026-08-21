use super::*;

pub(in super::super) fn validate_c_fixed_input(
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

pub(in super::super) fn validate_c_input_arrays(
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

pub(in super::super) fn validate_input_array_binding(
    manifest: &CAbiBindingManifest,
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
        let array = parameter
            .input_array
            .as_ref()
            .expect("filtered input array");
        if array.bytes && array.element_type.is_some() {
            return Err(format!(
                "error[native_bindgen.c_input_array_contract]: `{}` cannot combine bytes and opaque-resource element metadata",
                parameter.name
            ));
        }
        let expected = match c_pointer_base(&resolved) {
            "int64_t" => "List[Int]",
            "double" => "List[Float]",
            "uint8_t" if array.bytes => "Bytes",
            "uint8_t" => "List[Bool]",
            "uint64_t" => {
                let element_type = array.element_type.as_deref().ok_or_else(|| {
                    format!(
                        "error[native_bindgen.c_input_array_contract]: `{}` requires an opaque-resource element_type",
                        parameter.name
                    )
                })?;
                if binding_type(manifest, element_type).is_none() {
                    return Err(format!(
                        "error[native_bindgen.c_input_array_contract]: `{}` names unknown opaque-resource element type `{element_type}`",
                        parameter.name
                    ));
                }
                let expected = format!("List[{element_type}]");
                if argument.abi_ty() != expected {
                    return Err(format!(
                        "error[native_bindgen.c_input_array_contract]: `{}` requires `{expected}` for `{}`",
                        parameter.name, function.name
                    ));
                }
                continue;
            }
            base => {
                return Err(format!(
                    "error[native_bindgen.c_input_array_contract]: `{}` uses unsupported input element type `{base}`",
                    parameter.name
                ));
            }
        };
        if argument.abi_ty() != expected {
            return Err(format!(
                "error[native_bindgen.c_input_array_contract]: `{}` requires `{expected}` for `{}`",
                parameter.name, function.name
            ));
        }
    }
    Ok(())
}

pub(in super::super) fn validate_owned_string_binding(
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

pub(in super::super) fn validate_owned_array_binding(
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
