use super::*;

pub(super) fn inventory_tuple_layouts(
    ty: &AbiType,
    module_name: &str,
    layouts: &mut BTreeMap<String, Arc<[u8]>>,
) -> Result<(), String> {
    for argument in &ty.arguments {
        inventory_tuple_layouts(argument, module_name, layouts)?;
    }
    if ty.constructor != "$Tuple" {
        return Ok(());
    }
    let canonical = type_contract(ty, module_name)?;
    let descriptor = ManagedAggregateDescriptor::tuple(
        &canonical,
        ty.arguments
            .iter()
            .map(|field| managed_field_type_of(field, module_name))
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(|error| format!("error[self_host.native.tuple_layout]: {error}"))?;
    let encoded = Arc::<[u8]>::from(
        encode_aggregate_layout(&descriptor)
            .map_err(|error| format!("error[self_host.native.tuple_layout_abi]: {error}"))?,
    );
    if let Some(previous) = layouts.insert(canonical.clone(), encoded.clone()) {
        if previous.as_ref() != encoded.as_ref() {
            return Err(format!(
                "error[self_host.native.conflicting_tuple_layout]: semantic type {canonical:?} has multiple physical layouts"
            ));
        }
    }
    Ok(())
}

/// Inventories every concrete collection schema reachable from ABI type positions.
pub(in crate::compiler::native_ir) fn encoded_collection_layouts(
    envelopes: &[BackendEnvelope],
) -> Result<Vec<Arc<[u8]>>, String> {
    let mut layouts = BTreeMap::<String, Arc<[u8]>>::new();
    for envelope in envelopes {
        let module_name = envelope.module.name.as_str();
        for layout in &envelope.module.structs {
            for field in &layout.fields {
                if let Some(type_name) = field.type_name() {
                    inventory_collections(&parse_type(type_name)?, module_name, &mut layouts)?;
                }
            }
        }
        for function in &envelope.module.functions {
            for parameter in &function.parameters {
                inventory_collections(
                    &parse_type(&parameter.type_name)?,
                    module_name,
                    &mut layouts,
                )?;
            }
            inventory_collections(
                &parse_type(&function.return_type)?,
                module_name,
                &mut layouts,
            )?;
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                inventory_collections(
                    &parse_type(&instruction.type_name)?,
                    module_name,
                    &mut layouts,
                )?;
            }
        }
    }
    let mut layouts = layouts.into_values().collect::<Vec<_>>();
    layouts.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
    Ok(layouts)
}

/// Resolves one closed ABI type into its direct NativeIR value kind.
///
/// Bare non-primitive names deliberately return `None`: their defining module
/// must be supplied by authoritative type resolution before a semantic ID can
/// be assigned safely.
pub(in crate::compiler::native_ir) fn native_type(
    type_name: &str,
) -> Result<Option<NativeType>, String> {
    let ty = parse_type(type_name)?;
    native_type_of(&ty)
}

pub(super) fn native_type_of(ty: &AbiType) -> Result<Option<NativeType>, String> {
    if ty.constructor == "$Tuple" {
        return Ok(Some(NativeType::ManagedRef(
            SemanticTypeId::from_canonical(&type_contract(ty, "")?)
                .map_err(|error| error.to_string())?,
        )));
    }
    let short = ty.constructor.rsplit('.').next().unwrap_or(&ty.constructor);
    if ty.arguments.is_empty() {
        return Ok(match short {
            "Unit" => Some(NativeType::Unit),
            "Bool" => Some(NativeType::Bool),
            "Int" => Some(NativeType::Int),
            "Float" => Some(NativeType::Float),
            "Atom" => Some(NativeType::Atom),
            "String" => Some(NativeType::StringRef),
            "Bytes" => Some(NativeType::BytesRef),
            "Binary" | "BitString" => Some(NativeType::BinaryRef),
            "Dynamic" | "Term" => None,
            _ if ty.constructor.contains('.') => Some(NativeType::ManagedRef(
                SemanticTypeId::from_canonical(&nominal_contract(&ty.constructor, "")?)
                    .map_err(|error| error.to_string())?,
            )),
            _ => None,
        });
    }
    match (short, ty.arguments.as_slice()) {
        ("Process" | "Entry" | "Monitor" | "ResourceKind" | "Resource", [_]) => {
            Ok(Some(NativeType::Int))
        }
        ("Message", [payload]) => native_type_of(payload),
        _ if !context_free_closed(ty) => Ok(None),
        _ => Ok(Some(NativeType::ManagedRef(
            SemanticTypeId::from_canonical(&type_contract(&ty, "")?)
                .map_err(|error| error.to_string())?,
        ))),
    }
}

pub(super) fn context_free_closed(ty: &AbiType) -> bool {
    let short = ty.constructor.rsplit('.').next().unwrap_or(&ty.constructor);
    let constructor_is_closed = ty.constructor == "$Tuple"
        || ty.constructor.contains('.')
        || matches!(
            short,
            "Unit"
                | "Bool"
                | "Int"
                | "Float"
                | "Atom"
                | "String"
                | "Bytes"
                | "Binary"
                | "BitString"
                | "Never"
                | "List"
                | "Map"
                | "Set"
                | "Option"
                | "Result"
                | "Iterator"
                | "Process"
                | "Message"
                | "Entry"
                | "Monitor"
                | "ResourceKind"
                | "Resource"
        );
    constructor_is_closed && ty.arguments.iter().all(context_free_closed)
}

pub(super) fn lower_field(
    field: &IrStructField,
    module_name: &str,
    layout_name: &str,
) -> Result<(String, ManagedFieldType), String> {
    let IrStructField::Typed { name, type_name } = field else {
        return Err(format!(
            "error[self_host.native.legacy_struct_layout]: layout {layout_name:?} field {:?} has no ABI-1.2 type",
            field.name()
        ));
    };
    Ok((
        name.clone(),
        managed_field_type(type_name, module_name).map_err(|error| {
            format!(
                "error[self_host.native.struct_field_type]: layout {layout_name:?} field {name:?}: {error}"
            )
        })?,
    ))
}

pub(super) fn managed_field_type(
    type_name: &str,
    module_name: &str,
) -> Result<ManagedFieldType, String> {
    managed_field_type_of(&parse_type(type_name)?, module_name)
}

pub(super) fn parse_type(source: &str) -> Result<AbiType, String> {
    if source.len() > 4_096 {
        return Err("type contract exceeds the 4096-byte ABI bound".into());
    }
    parse_type_at(source.trim(), 0)
}

pub(super) fn abi_type_source(value: &AbiType) -> String {
    if value.constructor == "$Tuple" {
        return format!(
            "({})",
            value
                .arguments
                .iter()
                .map(abi_type_source)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if value.arguments.is_empty() {
        return value.constructor.clone();
    }
    format!(
        "{}[{}]",
        value.constructor,
        value
            .arguments
            .iter()
            .map(abi_type_source)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub(super) fn parse_type_at(source: &str, depth: usize) -> Result<AbiType, String> {
    if depth >= 64 {
        return Err("type contract exceeds the 64-level nesting bound".into());
    }
    if source.is_empty() {
        return Err("empty type contract".into());
    }
    if source.starts_with('(') && source.ends_with(')') {
        let inner = source[1..source.len() - 1].trim();
        let arguments = if inner.is_empty() {
            Vec::new()
        } else {
            split_type_arguments(inner)?
                .into_iter()
                .map(|argument| parse_type_at(argument, depth + 1))
                .collect::<Result<Vec<_>, _>>()?
        };
        return Ok(AbiType {
            constructor: "$Tuple".into(),
            arguments,
        });
    }
    let Some(open) = source.find('[') else {
        if is_nominal_identity(source) {
            return Ok(AbiType {
                constructor: source.into(),
                arguments: Vec::new(),
            });
        }
        return Err(format!("unsupported type contract {source:?}"));
    };
    if !source.ends_with(']') || !is_nominal_identity(source[..open].trim()) {
        return Err(format!("malformed type application {source:?}"));
    }
    let constructor = source[..open].trim();
    let arguments = split_type_arguments(&source[open + 1..source.len() - 1])?
        .into_iter()
        .enumerate()
        .map(|(index, argument)| {
            if constructor == "Array"
                && index == 0
                && !argument.is_empty()
                && argument.bytes().all(|byte| byte.is_ascii_digit())
            {
                Ok(AbiType {
                    constructor: argument.into(),
                    arguments: Vec::new(),
                })
            } else {
                parse_type_at(argument, depth + 1)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if arguments.is_empty() || arguments.len() > 256 {
        return Err(format!("invalid type application arity in {source:?}"));
    }
    Ok(AbiType {
        constructor: constructor.into(),
        arguments,
    })
}

pub(super) fn split_type_arguments(source: &str) -> Result<Vec<&str>, String> {
    let mut brackets = 0_usize;
    let mut parentheses = 0_usize;
    let mut braces = 0_usize;
    let mut start = 0_usize;
    let mut arguments = Vec::new();
    for (index, character) in source.char_indices() {
        match character {
            '[' => brackets = brackets.saturating_add(1),
            ']' => {
                brackets = brackets
                    .checked_sub(1)
                    .ok_or_else(|| "unbalanced type application".to_string())?;
            }
            '(' => parentheses = parentheses.saturating_add(1),
            ')' => {
                parentheses = parentheses
                    .checked_sub(1)
                    .ok_or_else(|| "unbalanced tuple type".to_string())?;
            }
            '{' => braces = braces.saturating_add(1),
            '}' => {
                braces = braces
                    .checked_sub(1)
                    .ok_or_else(|| "unbalanced record type".to_string())?;
            }
            ',' if brackets == 0 && parentheses == 0 && braces == 0 => {
                arguments.push(source[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    if brackets != 0 || parentheses != 0 || braces != 0 {
        return Err("unbalanced type application".into());
    }
    arguments.push(source[start..].trim());
    if arguments.iter().any(|argument| argument.is_empty()) {
        return Err("empty type application argument".into());
    }
    Ok(arguments)
}

pub(super) fn managed_field_type_of(
    ty: &AbiType,
    module_name: &str,
) -> Result<ManagedFieldType, String> {
    let short = ty.constructor.rsplit('.').next().unwrap_or(&ty.constructor);
    if ty.arguments.is_empty() {
        return Ok(match short {
            "Unit" => ManagedFieldType::Unit,
            "Bool" => ManagedFieldType::Bool,
            "Int" => ManagedFieldType::Int,
            "Float" => ManagedFieldType::Float,
            "Atom" => ManagedFieldType::Atom,
            "String" => reference("std.core.String")?,
            "Bytes" => reference("std.binary.Bytes")?,
            "Binary" | "BitString" => reference("std.binary.Binary")?,
            "Dynamic" | "Term" => {
                return Err(format!(
                    "open type {:?} has no closed managed layout",
                    ty.constructor
                ));
            }
            _ => reference(&type_contract(ty, module_name)?)?,
        });
    }
    match (short, ty.arguments.as_slice()) {
        ("Process" | "Entry" | "Monitor" | "ResourceKind" | "Resource", [_]) => {
            Ok(ManagedFieldType::Int)
        }
        ("Message", [payload]) => managed_field_type_of(payload, module_name),
        _ => reference(&type_contract(ty, module_name)?),
    }
}

pub(super) fn type_contract(ty: &AbiType, module_name: &str) -> Result<String, String> {
    if ty.constructor == "$Tuple" {
        return Ok(format!(
            "Tuple({})",
            ty.arguments
                .iter()
                .map(|argument| type_contract(argument, module_name))
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        ));
    }
    if ty.arguments.is_empty() {
        return match ty.constructor.rsplit('.').next().unwrap_or(&ty.constructor) {
            primitive @ ("Unit" | "Bool" | "Int" | "Float" | "Atom" | "String" | "Binary"
            | "Bytes" | "Never") => Ok(primitive.into()),
            _ => nominal_contract(&ty.constructor, module_name),
        };
    }
    if ty.constructor == "Array" {
        let [length, element] = ty.arguments.as_slice() else {
            return Err("Array requires a size and element type".into());
        };
        length
            .constructor
            .parse::<usize>()
            .map_err(|_| "Array size must be a concrete non-negative integer".to_string())?;
        if !length.arguments.is_empty() {
            return Err("Array size must not have type arguments".into());
        }
        return Ok(format!(
            "Apply({};{},{})",
            ty.constructor,
            length.constructor,
            type_contract(element, module_name)?
        ));
    }
    let arguments = ty
        .arguments
        .iter()
        .map(|argument| type_contract(argument, module_name))
        .collect::<Result<Vec<_>, _>>()?
        .join(",");
    Ok(format!("Apply({};{arguments})", ty.constructor))
}

pub(super) fn inventory_collections(
    ty: &AbiType,
    module_name: &str,
    layouts: &mut BTreeMap<String, Arc<[u8]>>,
) -> Result<(), String> {
    for argument in &ty.arguments {
        inventory_collections(argument, module_name, layouts)?;
    }
    let short = ty.constructor.rsplit('.').next().unwrap_or(&ty.constructor);
    let descriptor = match (short, ty.arguments.as_slice()) {
        ("List", [element]) => Some(ManagedCollectionDescriptor::list(
            &type_contract(ty, module_name)?,
            managed_field_type_of(element, module_name)?,
        )),
        ("Map", [key, value]) => Some(ManagedCollectionDescriptor::map(
            &type_contract(ty, module_name)?,
            managed_field_type_of(key, module_name)?,
            managed_field_type_of(value, module_name)?,
        )),
        ("Set", [element]) => Some(ManagedCollectionDescriptor::set(
            &type_contract(ty, module_name)?,
            managed_field_type_of(element, module_name)?,
        )),
        ("List" | "Map" | "Set", _) => {
            return Err(format!(
                "error[self_host.native.collection_arity]: invalid collection type {:?}",
                ty.constructor
            ));
        }
        _ => None,
    };
    let Some(descriptor) = descriptor else {
        return Ok(());
    };
    let descriptor = descriptor
        .map_err(|error| format!("error[self_host.native.collection_layout]: {error}"))?;
    let canonical = descriptor.canonical_type().to_owned();
    let encoded = Arc::<[u8]>::from(
        encode_collection_layout(&descriptor)
            .map_err(|error| format!("error[self_host.native.collection_layout]: {error}"))?,
    );
    if let Some(previous) = layouts.insert(canonical.clone(), encoded.clone()) {
        if previous.as_ref() != encoded.as_ref() {
            return Err(format!(
                "error[self_host.native.conflicting_collection_layout]: semantic type {canonical:?} has multiple physical layouts"
            ));
        }
    }
    Ok(())
}

pub(super) fn reference(canonical: &str) -> Result<ManagedFieldType, String> {
    SemanticTypeId::from_canonical(canonical)
        .map(ManagedFieldType::Reference)
        .map_err(|error| error.to_string())
}

pub(super) fn nominal_contract(identity: &str, module_name: &str) -> Result<String, String> {
    let identity = identity.trim();
    if !is_nominal_identity(identity) {
        return Err(format!("invalid nominal type identity {identity:?}"));
    }
    let qualified = if identity.contains('.') || module_name.is_empty() {
        identity.to_owned()
    } else {
        format!("{module_name}.{identity}")
    };
    Ok(format!("Named({qualified})"))
}

pub(super) fn is_nominal_identity(identity: &str) -> bool {
    !identity.is_empty()
        && identity.split('.').all(|part| {
            let mut characters = part.chars();
            characters
                .next()
                .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
                && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
        })
}
