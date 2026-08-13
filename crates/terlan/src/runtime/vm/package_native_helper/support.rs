use super::*;

pub(super) fn accelerator_resource_handles(
    value: &ReplValue,
) -> VmRuntimeResult<Vec<AcceleratorResourceHandle>> {
    let mut handles = Vec::new();
    collect_accelerator_resource_handles(value, &mut handles)?;
    Ok(handles)
}

fn collect_accelerator_resource_handles(
    value: &ReplValue,
    handles: &mut Vec<AcceleratorResourceHandle>,
) -> VmRuntimeResult<()> {
    match value {
        ReplValue::Record { fields, .. } => {
            if let Some(handle) = accelerator_resource_handle(fields)? {
                handles.push(handle);
                return Ok(());
            }
            for (_, field) in fields {
                collect_accelerator_resource_handles(field, handles)?;
            }
        }
        ReplValue::List(values) | ReplValue::Tuple(values) => {
            for value in values {
                collect_accelerator_resource_handles(value, handles)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn accelerator_resource_handle(
    fields: &[(String, ReplValue)],
) -> VmRuntimeResult<Option<AcceleratorResourceHandle>> {
    let text = |name: &str| {
        fields
            .iter()
            .find(|(field, _)| field == name)
            .and_then(|(_, value)| match value {
                ReplValue::String(value) => Some(value.as_str()),
                _ => None,
            })
    };
    let integer = |name: &str| {
        fields
            .iter()
            .find(|(field, _)| field == name)
            .and_then(|(_, value)| match value {
                ReplValue::Int(value) => u64::try_from(*value).ok(),
                _ => None,
            })
    };
    let Some(type_name) = text("$native_type") else {
        return Ok(None);
    };
    let Some(class) = accelerator_resource_class(type_name) else {
        return Ok(None);
    };
    let owner = text("$native_owner")
        .ok_or_else(|| "error[accelerator.resource_handle]: missing native owner".to_string())?;
    let slot = integer("$native_id")
        .ok_or_else(|| "error[accelerator.resource_handle]: invalid native slot".to_string())?;
    let generation = integer("$native_generation").ok_or_else(|| {
        "error[accelerator.resource_handle]: invalid native generation".to_string()
    })?;
    let provider = type_name
        .split('.')
        .next()
        .unwrap_or("accelerator")
        .to_ascii_lowercase();
    let space = accelerator_class_name(class).to_string();
    let principal =
        AcceleratorResourcePrincipal::new(canonical_resource_principal(&provider, owner)?)
            .map_err(|error| {
                format!("error[accelerator.resource_handle]: invalid owner: {error}")
            })?;
    let handle = AcceleratorResourceHandle {
        id: AcceleratorResourceId { slot, generation },
        class,
        address_space: AcceleratorAddressSpace::External { provider, space },
        role: AcceleratorResourceRole::Owned { principal },
    };
    handle
        .validate()
        .map_err(|error| format!("error[accelerator.resource_handle]: {error}"))?;
    Ok(Some(handle))
}

fn canonical_resource_principal(provider: &str, owner: &str) -> VmRuntimeResult<String> {
    if owner.is_empty() || owner.len() > 256 {
        return Err("error[accelerator.resource_handle]: invalid native owner token".into());
    }
    let encoded = owner
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("{provider}.{encoded}"))
}

fn accelerator_resource_class(type_name: &str) -> Option<AcceleratorResourceClass> {
    match type_name.rsplit('.').next()? {
        "Device" => Some(AcceleratorResourceClass::DeviceContext),
        "Buffer" | "HostBuffer" => Some(AcceleratorResourceClass::Allocation),
        "Stream" => Some(AcceleratorResourceClass::Stream),
        "Event" => Some(AcceleratorResourceClass::Event),
        "Module" => Some(AcceleratorResourceClass::Module),
        "Kernel" => Some(AcceleratorResourceClass::Kernel),
        "Graph" => Some(AcceleratorResourceClass::Graph),
        "Communicator" => Some(AcceleratorResourceClass::Communicator),
        "Tensor" => Some(AcceleratorResourceClass::ImportedTensor),
        _ => None,
    }
}

fn accelerator_class_name(class: AcceleratorResourceClass) -> &'static str {
    match class {
        AcceleratorResourceClass::DeviceContext => "device-context",
        AcceleratorResourceClass::Allocation => "allocation",
        AcceleratorResourceClass::Stream => "stream",
        AcceleratorResourceClass::Event => "event",
        AcceleratorResourceClass::Module => "module",
        AcceleratorResourceClass::Kernel => "kernel",
        AcceleratorResourceClass::Graph => "graph",
        AcceleratorResourceClass::Communicator => "communicator",
        AcceleratorResourceClass::ImportedTensor => "imported-tensor",
    }
}

pub(super) fn result_ok(value: ReplValue) -> ReplValue {
    ReplValue::Record {
        name: "Ok".to_string(),
        fields: vec![("value".to_string(), value)],
    }
}

pub(super) fn result_error(code: String, message: String) -> ReplValue {
    ReplValue::Record {
        name: "Err".to_string(),
        fields: vec![(
            "reason".to_string(),
            ReplValue::Record {
                name: "Error".to_string(),
                fields: vec![
                    ("code".to_string(), ReplValue::Atom(code)),
                    ("message".to_string(), ReplValue::String(message)),
                ],
            },
        )],
    }
}

pub(super) fn decode_string_rows(value: &str) -> VmRuntimeResult<Vec<ReplValue>> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|row| {
            let row = STANDARD
                .decode(row)
                .map_err(|error| format!("error[native_helper_protocol]: {error}"))?;
            let row = String::from_utf8(row)
                .map_err(|error| format!("error[native_helper_protocol]: {error}"))?;
            let (length, values) = row.split_once('|').ok_or_else(|| {
                "error[native_helper_protocol]: string row length is missing".to_string()
            })?;
            let length = length
                .parse::<usize>()
                .map_err(|error| format!("error[native_helper_protocol]: {error}"))?;
            let values = if values.is_empty() {
                Vec::new()
            } else {
                values
                    .split(',')
                    .map(|value| decode_text(value).map(ReplValue::String))
                    .collect::<Result<Vec<_>, _>>()?
            };
            if values.len() != length {
                return Err(
                    "error[native_helper_protocol]: string row length does not match payload"
                        .into(),
                );
            }
            Ok(ReplValue::List(values))
        })
        .collect()
}

pub(super) fn parse_list<T>(
    values: &str,
    mut parse: impl FnMut(&str) -> Result<ReplValue, T>,
) -> VmRuntimeResult<ReplValue>
where
    T: std::fmt::Display,
{
    values
        .split(',')
        .map(|value| {
            parse(value).map_err(|error| {
                VmRuntimeError::message(format!("error[native_helper_protocol]: {error}"))
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(ReplValue::List)
}

pub(super) fn decode_record_fields(value: &str) -> VmRuntimeResult<Vec<(String, ReplValue)>> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|field| {
            let mut parts = field.splitn(3, ':');
            let name = decode_text(parts.next().ok_or_else(|| {
                "error[native_helper_protocol]: record field name is missing".to_string()
            })?)?;
            let kind = parts.next().ok_or_else(|| {
                "error[native_helper_protocol]: record field kind is missing".to_string()
            })?;
            let value = parts.next().ok_or_else(|| {
                "error[native_helper_protocol]: record field value is missing".to_string()
            })?;
            let value = match kind {
                "i" => value
                    .parse::<i64>()
                    .map(ReplValue::Int)
                    .map_err(|error| format!("error[native_helper_protocol]: {error}"))?,
                "f" => value
                    .parse::<f64>()
                    .map(|value| ReplValue::Float(value.to_string()))
                    .map_err(|error| format!("error[native_helper_protocol]: {error}"))?,
                "b" => value
                    .parse::<bool>()
                    .map(ReplValue::Bool)
                    .map_err(|error| format!("error[native_helper_protocol]: {error}"))?,
                _ => {
                    return Err(format!(
                        "error[native_helper_protocol]: unsupported record field kind `{kind}`"
                    )
                    .into())
                }
            };
            Ok((name, value))
        })
        .collect()
}

pub(super) fn decode_text(value: &str) -> VmRuntimeResult<String> {
    let bytes = STANDARD
        .decode(value)
        .map_err(|error| format!("error[native_helper_protocol]: {error}"))?;
    Ok(String::from_utf8(bytes)
        .map_err(|error| format!("error[native_helper_protocol]: {error}"))?)
}

pub(super) fn parse_u64(value: &str) -> VmRuntimeResult<u64> {
    Ok(value
        .parse::<u64>()
        .map_err(|error| format!("error[native_helper_protocol]: {error}"))?)
}
