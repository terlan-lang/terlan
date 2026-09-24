//! Concrete nominal record layouts and typed record construction.

use super::*;
use crate::terlan_typeck::CoreTypeDecl;

/// Adds syntax-level struct records, which have direct record construction but
/// no constructor declaration in CoreIR.
pub(in crate::compiler::native_ir) fn install_struct_layouts(
    modules: &[(&str, &[CoreTypeDecl])],
    consumer_module: &str,
    layouts: &mut NativeConstructorLayouts,
) -> Result<(), String> {
    for (module, declarations) in modules {
        for declaration in *declarations {
            if !declaration.params.is_empty() {
                continue;
            }
            let Some(CoreType::Struct { name, fields }) = declaration.core_body.as_ref() else {
                continue;
            };
            let parameters = fields
                .iter()
                .map(|field| native_type(Some(&field.ty), &field.ty.contract_text()))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.struct_layout_type]: struct `{name}` has an unsupported field"
                    )
                })?;
            let descriptor = Arc::new(
                ManagedAggregateDescriptor::record(
                    name,
                    fields
                        .iter()
                        .zip(parameters.iter().copied())
                        .map(|(field, ty)| {
                            managed_field_type(ty).map(|ty| (field.name.clone(), ty))
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                )
                .map_err(|error| format!("error[native_ir.struct_layout]: {error}"))?,
            );
            let encoded_layout = Arc::<[u8]>::from(
                encode_aggregate_layout(&descriptor)
                    .map_err(|error| format!("error[native_ir.struct_layout_abi]: {error}"))?,
            );
            let layout = NativeConstructorLayout {
                parameter_core_types: fields.iter().map(|field| Some(field.ty.clone())).collect(),
                parameters,
                result: NativeType::ManagedRef(
                    SemanticTypeId::from_canonical(name)
                        .map_err(|error| format!("error[native_ir.struct_layout]: {error}"))?,
                ),
                result_core_type: declaration.core_body.clone(),
                descriptor,
                encoded_layout,
            };
            let qualified = (name.clone(), fields.len());
            if layouts.contains_key(&qualified) {
                continue;
            }
            layouts.insert(qualified, layout.clone());
            if *module == consumer_module {
                layouts.insert((declaration.name.clone(), fields.len()), layout);
            }
        }
    }
    Ok(())
}

/// Lowers a generic named record using the concrete checked cast target.
///
/// The target selects a registered concrete layout. Its substituted field
/// types supply context even for empty collections and nullary union values.
/// Ordinary and parameterized records keep the same identity as image metadata.
pub(in crate::compiler::native_ir) fn lower_structural_record_construct(
    expr: &CoreExpr,
    target: &CoreType,
    layouts: &NativeConstructorLayouts,
    lower_field: impl Fn(&CoreExpr, &CoreType) -> Result<(NativeExpr, NativeType), String>,
) -> Result<Option<NativeExpr>, String> {
    let CoreExpr::RecordConstruct { name, fields } = expr else {
        return Ok(None);
    };
    if let CoreType::Union(variants) = target {
        if !variants.iter().any(|variant| matches!(variant, CoreType::Struct { name: identity, .. } if identity == name)) {
            return Err(format!("error[native_ir.record_identity]: `{name}` is not a declared record variant"));
        }
        let canonical = super::super::expression::managed_semantic_contract(target);
        let short = name.rsplit('.').next().unwrap_or(name);
        let key = (format!("$structural.{canonical}.{short}"), fields.len());
        let template = layouts.get(&key).ok_or_else(|| {
            format!(
                "error[native_ir.structural_record_shape]: no admitted union layout for `{name}`"
            )
        })?;
        let mut source = HashMap::new();
        for field in fields {
            if source.insert(field.key.as_str(), &field.value).is_some() {
                return Err(format!(
                    "error[native_ir.record_field_duplicate]: record `{name}` repeats field `{}`",
                    field.key
                ));
            }
        }
        let lowered = template.descriptor.fields().iter().enumerate().map(|(index, field)| {
            let value = field.name().and_then(|name| source.get(name)).ok_or_else(|| format!("error[native_ir.structural_record_field]: missing union record field in `{name}`"))?;
            let expected = template.parameter_core_types[index].as_ref().ok_or("error[native_ir.structural_record_field_type]: missing union field type")?;
            let (value, ty) = lower_field(value, expected)?;
            if ty != template.parameters[index] {
                return Err("error[native_ir.structural_record_field_type]: incompatible union record field".into());
            }
            Ok(value)
        }).collect::<Result<Vec<_>, String>>()?;
        return Ok(Some(NativeExpr::Construct {
            descriptor: Arc::clone(&template.descriptor),
            encoded_layout: Arc::clone(&template.encoded_layout),
            fields: lowered,
        }));
    }
    let target_name = match target {
        CoreType::Apply { constructor, .. } | CoreType::Named(constructor) => constructor,
        CoreType::Struct { name, .. } => name,
        _ => return Ok(None),
    };
    if target_name.rsplit('.').next() != name.rsplit('.').next() {
        return Ok(None);
    }
    let canonical = target.contract_text();
    let template = match layouts.get(&(canonical, fields.len())) {
        Some(layout) => layout,
        None => record_layout(name, fields.len(), layouts)?,
    };
    let mut source = HashMap::new();
    for field in fields {
        if source.insert(field.key.as_str(), &field.value).is_some() {
            return Err(format!(
                "error[native_ir.record_field_duplicate]: record `{name}` repeats field `{}`",
                field.key
            ));
        }
    }
    let mut lowered = Vec::with_capacity(fields.len());
    let mut descriptor_fields = Vec::with_capacity(fields.len());
    for (index, expected) in template.descriptor.fields().iter().enumerate() {
        let field_name = expected.name().ok_or_else(|| {
            format!(
                "error[native_ir.structural_record_shape]: record `{name}` has an unnamed field"
            )
        })?;
        let value = source.get(field_name).ok_or_else(|| {
            format!("error[native_ir.structural_record_field]: record `{name}` is missing field `{field_name}`")
        })?;
        let expected_core = template.parameter_core_types.get(index).and_then(Option::as_ref).ok_or_else(|| {
            format!("error[native_ir.structural_record_field_type]: record `{name}` field `{field_name}` has no checked type")
        })?;
        let (value, ty) = lower_field(value, expected_core)?;
        descriptor_fields.push((field_name.to_string(), managed_field_type(ty)?));
        lowered.push(value);
    }
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::record(template.descriptor.canonical_type(), descriptor_fields)
            .map_err(|error| format!("error[native_ir.structural_record_layout]: {error}"))?,
    );
    let encoded_layout = Arc::<[u8]>::from(
        encode_aggregate_layout(&descriptor)
            .map_err(|error| format!("error[native_ir.structural_record_abi]: {error}"))?,
    );
    Ok(Some(NativeExpr::Construct {
        descriptor,
        encoded_layout,
        fields: lowered,
    }))
}
