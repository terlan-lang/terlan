//! Canonical managed-constructor metadata for direct AOT lowering.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_field_operation, encode_aggregate_layout,
    encode_aggregate_scalar_field_operation, ManagedAggregateDescriptor, SemanticTypeId,
};
use crate::terlan_typeck::{
    core_type_from_text, CoreConstructorDecl, CoreExpr, CoreRecordExprField, CoreTupleTypeElem,
    CoreType, CoreTypeDecl,
};

use super::{call_composition::rebase_callee_locals, native_type, NativeExpr, NativeType};

mod field_types;
pub(super) use field_types::managed_field_type;
use field_types::native_field_type;

mod structural_registry;
pub(super) use structural_registry::install_structural_type_layouts;

/// One fixed constructor admitted to managed NativeIR.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NativeConstructorLayout {
    /// Native parameter kinds in source order.
    pub(super) parameters: Vec<NativeType>,
    /// Checked source types retained for representation-aware field lowering.
    pub(super) parameter_core_types: Vec<Option<CoreType>>,
    /// Exact managed result kind shared by every variant in the union.
    pub(super) result: NativeType,
    /// Checked semantic result retained without reparsing runtime descriptors.
    pub(super) result_core_type: Option<CoreType>,
    /// Canonical runtime descriptor for this active variant.
    pub(super) descriptor: Arc<ManagedAggregateDescriptor>,
    /// Bounded immutable descriptor bytes passed through the allocation ABI.
    pub(super) encoded_layout: Arc<[u8]>,
}

/// Constructor identities resolved for one application module.
pub(super) type NativeConstructorLayouts = HashMap<(String, usize), NativeConstructorLayout>;

/// Normalizes compiler-owned facade fields that have one public managed ABI.
pub(super) fn canonical_structural_field_name(
    canonical: &str,
    variant: &str,
    index: usize,
    source_name: Option<&str>,
) -> Option<String> {
    if canonical == "Apply(Option;String)" && variant == "Some" && index == 0 {
        Some("value".to_string())
    } else {
        source_name.map(str::to_string)
    }
}

/// Recovers the checked semantic type carried by a managed native value.
///
/// Continuation lowering retains the compact native representation for local
/// variables. Collection literals assembled from those variables still need
/// the original semantic element type so their List schema remains identical
/// to the checked program rather than becoming an opaque managed collection.
pub(super) fn result_core_type_for_native(
    result: NativeType,
    layouts: &NativeConstructorLayouts,
) -> Option<CoreType> {
    let mut matches = layouts
        .values()
        .filter(|layout| layout.result == result)
        .filter_map(|layout| layout.result_core_type.clone());
    let first = matches.next()?;
    matches.all(|candidate| candidate == first).then_some(first)
}

/// Builds deterministic fixed-constructor layouts visible from one module.
pub(super) fn native_constructor_layouts(
    modules: &[(&str, &[CoreConstructorDecl])],
    consumer_module: &str,
) -> Result<NativeConstructorLayouts, String> {
    let mut variants = Vec::new();
    let mut blocked_groups = HashSet::new();
    for (module, declarations) in modules {
        for declaration in *declarations {
            let Some(return_core) = declaration.core_return_type.as_ref() else {
                continue;
            };
            let Some(result) = native_type(Some(return_core), &declaration.return_type) else {
                continue;
            };
            if !result.is_managed_reference() {
                continue;
            }
            // Transparent records are nominal managed values even though
            // their field shape is visible. Their ordinary record literal
            // layout and imported function signatures use the qualified
            // record name, so an explicit `pub constructor` must use that
            // same semantic identity instead of hashing `Struct(name;...)`.
            let is_record = matches!(return_core, CoreType::Struct { .. });
            let canonical = super::expression::managed_semantic_contract(return_core);
            let group = ((*module).to_owned(), canonical.clone());
            let parameters = declaration
                .params
                .iter()
                .map(|parameter| native_type(parameter.core_ty.as_ref(), &parameter.ty))
                .collect::<Option<Vec<_>>>();
            let Some(parameters) = parameters.filter(|parameters| {
                declaration.vararg.is_none() && parameters.len() == declaration.min_arity
            }) else {
                blocked_groups.insert(group);
                continue;
            };
            variants.push((
                (*module).to_owned(),
                declaration,
                canonical,
                result,
                parameters,
                is_record,
            ));
        }
    }
    variants.retain(|(module, _, canonical, _, _, _)| {
        !blocked_groups.contains(&(module.clone(), canonical.clone()))
    });
    variants.sort_by(|left, right| {
        left.2
            .cmp(&right.2)
            .then_with(|| left.0.cmp(&right.0))
            .then_with(|| left.1.name.cmp(&right.1.name))
            .then_with(|| left.1.min_arity.cmp(&right.1.min_arity))
    });

    let mut group_sizes = HashMap::<(String, String), u32>::new();
    for (module, _, canonical, _, _, _) in &variants {
        let count = group_sizes
            .entry((module.clone(), canonical.clone()))
            .or_default();
        *count = count.checked_add(1).ok_or_else(|| {
            "error[native_ir.constructor_variants]: constructor union exceeds u32 capacity"
                .to_string()
        })?;
    }
    let mut discriminants = HashMap::<(String, String), u32>::new();
    let mut layouts = HashMap::new();
    for (module, declaration, canonical, result, parameters, is_record) in variants {
        let group = (module.clone(), canonical.clone());
        let discriminant = discriminants.entry(group.clone()).or_default();
        let field_types = parameters
            .iter()
            .copied()
            .map(managed_field_type)
            .collect::<Result<Vec<_>, _>>()?;
        let fields = declaration
            .params
            .iter()
            .map(|parameter| parameter.name.clone())
            .zip(field_types)
            .collect::<Vec<_>>();
        let descriptor = Arc::new(
            if is_record {
                ManagedAggregateDescriptor::record(&canonical, fields)
            } else {
                ManagedAggregateDescriptor::constructor(
                    &canonical,
                    &declaration.name,
                    *discriminant,
                    group_sizes[&group],
                    fields
                        .into_iter()
                        .enumerate()
                        .map(|(index, (name, ty))| {
                            (
                                canonical_structural_field_name(
                                    &canonical,
                                    &declaration.name,
                                    index,
                                    Some(&name),
                                ),
                                ty,
                            )
                        })
                        .collect(),
                )
            }
            .map_err(|error| format!("error[native_ir.constructor_layout]: {error}"))?,
        );
        *discriminant += 1;
        let identity = format!("{module}.{}", declaration.name);
        let key = (identity, declaration.min_arity);
        let encoded_layout = Arc::<[u8]>::from(
            encode_aggregate_layout(&descriptor)
                .map_err(|error| format!("error[native_ir.constructor_abi]: {error}"))?,
        );
        let layout = NativeConstructorLayout {
            parameter_core_types: declaration
                .params
                .iter()
                .map(|parameter| {
                    parameter
                        .core_ty
                        .clone()
                        .or_else(|| core_type_from_text(&parameter.ty))
                })
                .collect(),
            parameters,
            result,
            result_core_type: declaration.core_return_type.clone(),
            descriptor,
            encoded_layout,
        };
        if layouts.insert(key.clone(), layout.clone()).is_some() {
            return Err(format!(
                "error[native_ir.constructor_duplicate]: duplicate constructor `{}/{}`",
                key.0, key.1
            ));
        }
        if module == consumer_module {
            layouts.insert((declaration.name.clone(), declaration.min_arity), layout);
        }
    }
    Ok(layouts)
}

/// Adds syntax-level struct records, which have direct record construction but
/// no constructor declaration in CoreIR.
pub(super) fn install_struct_layouts(
    modules: &[(&str, &[CoreTypeDecl])],
    consumer_module: &str,
    layouts: &mut NativeConstructorLayouts,
) -> Result<(), String> {
    for (module, declarations) in modules {
        for declaration in *declarations {
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

/// Resolves and lowers one checked fixed-constructor call.
pub(super) fn lower_constructor_call(
    expr: &CoreExpr,
    layouts: &NativeConstructorLayouts,
    lower_field: impl Fn(&CoreExpr, Option<&CoreType>) -> Result<(NativeExpr, NativeType), String>,
) -> Result<Option<(NativeExpr, NativeType)>, String> {
    let CoreExpr::ConstructorCall {
        constructor,
        constructor_identity,
        args,
    } = expr
    else {
        return Ok(None);
    };
    let identity = constructor_identity.as_deref().unwrap_or(constructor);
    let layout = layouts
        .get(&(identity.to_owned(), args.len()))
        .ok_or_else(|| {
            format!(
                "error[native_ir.constructor]: fixed constructor `{identity}/{}` has no native layout",
                args.len()
            )
        })?;
    let fields = args
        .iter()
        .zip(&layout.parameters)
        .zip(&layout.parameter_core_types)
        .enumerate()
        .map(|(index, ((argument, expected), expected_core))| {
            if let Some(field) = lower_zero_field_managed_variant(argument, *expected, layouts)? {
                return Ok(field);
            }
            let (field, actual) = lower_field(argument, expected_core.as_ref())?;
            if actual != *expected {
                return Err(format!(
                    "error[native_ir.constructor_field]: constructor `{identity}` field {index} requires {expected:?}, found {actual:?}"
                ));
            }
            Ok(field)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some((
        NativeExpr::Construct {
            descriptor: layout.descriptor.clone(),
            encoded_layout: layout.encoded_layout.clone(),
            fields,
        },
        layout.result,
    )))
}

/// Reifies an atom-form zero-field variant when a containing aggregate field
/// requires the managed union representation.
pub(super) fn lower_zero_field_managed_variant(
    argument: &CoreExpr,
    expected: NativeType,
    layouts: &NativeConstructorLayouts,
) -> Result<Option<NativeExpr>, String> {
    let NativeType::ManagedRef(_) = expected else {
        return Ok(None);
    };
    let identity = match argument {
        CoreExpr::Atom(identity) | CoreExpr::Var(identity) => identity,
        CoreExpr::ConstructorCall {
            constructor, args, ..
        } if args.is_empty() => constructor,
        _ => return Ok(None),
    };
    let identity = identity.rsplit('.').next().unwrap_or(identity);
    let mut candidates = layouts.values().filter(|layout| {
        layout.result == expected
            && layout.parameters.is_empty()
            && layout
                .descriptor
                .variant_name()
                .is_some_and(|variant| variant.eq_ignore_ascii_case(identity))
    });
    let Some(candidate) = candidates.next() else {
        return Ok(None);
    };
    if candidates.any(|other| other.encoded_layout != candidate.encoded_layout) {
        return Err(format!(
            "error[native_ir.constructor_variant]: atom `{identity}` has ambiguous managed layouts"
        ));
    }
    Ok(Some(NativeExpr::Construct {
        descriptor: candidate.descriptor.clone(),
        encoded_layout: candidate.encoded_layout.clone(),
        fields: Vec::new(),
    }))
}

/// Lowers a structural Option/Result constructor using its checked target type.
pub(super) fn lower_structural_constructor_call(
    expr: &CoreExpr,
    target: &CoreType,
    lower_field: impl Fn(&CoreExpr, &CoreType) -> Result<(NativeExpr, NativeType), String>,
) -> Result<Option<NativeExpr>, String> {
    let CoreExpr::ConstructorCall {
        constructor, args, ..
    } = expr
    else {
        return Ok(None);
    };
    let name = constructor.rsplit('.').next().unwrap_or(constructor);
    let Some((discriminant, variant_count, fields)) = structural_constructor_fields(name, target)
    else {
        return Ok(None);
    };
    if args.len() != fields.len() {
        return Err(format!(
            "error[native_ir.structural_constructor_arity]: `{name}` expects {} fields",
            fields.len()
        ));
    }
    let mut lowered = Vec::with_capacity(fields.len());
    let mut descriptor_fields = Vec::with_capacity(fields.len());
    for (index, (argument, (field_name, field_type))) in args.iter().zip(&fields).enumerate() {
        let expected =
            native_type(Some(field_type), &field_type.contract_text()).ok_or_else(|| {
                format!(
                    "error[native_ir.structural_constructor_type]: unsupported `{}`",
                    field_type.contract_text()
                )
            })?;
        let (field, actual) = lower_field(argument, field_type)?;
        if actual != expected {
            return Err(format!(
                "error[native_ir.structural_constructor_field]: `{name}` field {index} requires {expected:?}, found {actual:?}"
            ));
        }
        lowered.push(field);
        descriptor_fields.push((field_name.clone(), managed_field_type(expected)?));
    }
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::constructor(
            &super::expression::managed_semantic_contract(target),
            name,
            discriminant,
            variant_count,
            descriptor_fields,
        )
        .map_err(|error| format!("error[native_ir.structural_constructor_layout]: {error}"))?,
    );
    let encoded_layout = Arc::<[u8]>::from(
        encode_aggregate_layout(&descriptor)
            .map_err(|error| format!("error[native_ir.structural_constructor_abi]: {error}"))?,
    );
    Ok(Some(NativeExpr::Construct {
        descriptor,
        encoded_layout,
        fields: lowered,
    }))
}

/// Lowers a generic named record using the concrete checked cast target.
///
/// Generic struct declarations retain type parameters in the application-wide
/// layout template. At a monomorphized construction site the cast target owns
/// the concrete semantic identity, while the already checked field values own
/// the concrete physical kinds. Combining those two facts produces one exact
/// managed layout without weakening runtime type validation.
pub(super) fn lower_structural_record_construct(
    expr: &CoreExpr,
    target: &CoreType,
    layouts: &NativeConstructorLayouts,
    lower_field: impl Fn(&CoreExpr) -> Result<(NativeExpr, NativeType), String>,
) -> Result<Option<NativeExpr>, String> {
    let CoreExpr::RecordConstruct { name, fields } = expr else {
        return Ok(None);
    };
    let target_name = match target {
        CoreType::Apply { constructor, .. } | CoreType::Named(constructor) => constructor,
        CoreType::Struct { name, .. } => name,
        _ => return Ok(None),
    };
    if target_name.rsplit('.').next() != name.rsplit('.').next() {
        return Ok(None);
    }
    let template = record_layout(name, fields.len(), layouts)?;
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
    for expected in template.descriptor.fields() {
        let field_name = expected.name().ok_or_else(|| {
            format!(
                "error[native_ir.structural_record_shape]: record `{name}` has an unnamed field"
            )
        })?;
        let value = source.get(field_name).ok_or_else(|| {
            format!("error[native_ir.structural_record_field]: record `{name}` is missing field `{field_name}`")
        })?;
        let (value, ty) = lower_field(value)?;
        descriptor_fields.push((field_name.to_string(), managed_field_type(ty)?));
        lowered.push(value);
    }
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::record(&target.contract_text(), descriptor_fields)
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

type StructuralFields = Vec<(Option<String>, CoreType)>;

pub(super) fn structural_constructor_fields(
    name: &str,
    target: &CoreType,
) -> Option<(u32, u32, StructuralFields)> {
    match target {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Option") && args.len() == 1 =>
        {
            match name {
                "None" => Some((0, 2, Vec::new())),
                "Some" => Some((1, 2, vec![(Some("value".to_string()), args[0].clone())])),
                _ => None,
            }
        }
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Result") && args.len() == 2 =>
        {
            match name {
                "Ok" => Some((0, 2, vec![(Some("value".to_string()), args[0].clone())])),
                "Err" => Some((1, 2, vec![(Some("reason".to_string()), args[1].clone())])),
                _ => None,
            }
        }
        CoreType::Union(variants) => variants.iter().enumerate().find_map(|(index, variant)| {
            if name == "None" && matches!(variant, CoreType::AtomLiteral(atom) if atom == "none") {
                return Some((
                    u32::try_from(index).ok()?,
                    u32::try_from(variants.len()).ok()?,
                    Vec::new(),
                ));
            }
            let CoreType::Tuple(elements) = variant else {
                return None;
            };
            let (first, fields) = elements.split_first()?;
            let atom = match first {
                crate::terlan_typeck::CoreTupleTypeElem::Type(CoreType::AtomLiteral(atom))
                | crate::terlan_typeck::CoreTupleTypeElem::Field {
                    ty: CoreType::AtomLiteral(atom),
                    ..
                } => atom,
                _ => return None,
            };
            if !((name == "Err" && atom == "error") || name.eq_ignore_ascii_case(atom)) {
                return None;
            }
            Some((
                u32::try_from(index).ok()?,
                u32::try_from(variants.len()).ok()?,
                fields
                    .iter()
                    .map(|field| match field {
                        crate::terlan_typeck::CoreTupleTypeElem::Type(ty) => (None, ty.clone()),
                        crate::terlan_typeck::CoreTupleTypeElem::Field { name, ty } => {
                            (Some(name.clone()), ty.clone())
                        }
                    })
                    .collect(),
            ))
        }),
        CoreType::Tuple(elements) => {
            let CoreTupleTypeElem::Type(CoreType::AtomLiteral(tag)) = elements.first()? else {
                return None;
            };
            if constructor_tag(name) != *tag {
                return None;
            }
            let fields = elements
                .iter()
                .skip(1)
                .map(|element| match element {
                    CoreTupleTypeElem::Type(ty) => (None, ty.clone()),
                    CoreTupleTypeElem::Field { name, ty } => (Some(name.clone()), ty.clone()),
                })
                .collect();
            Some((0, 1, fields))
        }
        _ => None,
    }
}

fn constructor_tag(name: &str) -> String {
    let mut tag = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_uppercase() {
            if index != 0 {
                tag.push('_');
            }
            tag.extend(character.to_lowercase());
        } else {
            tag.push(character);
        }
    }
    tag
}

/// Returns the exact result kind for one resolved constructor call.
pub(super) fn constructor_result_type(
    expr: &CoreExpr,
    layouts: &NativeConstructorLayouts,
) -> Option<NativeType> {
    let CoreExpr::ConstructorCall {
        constructor,
        constructor_identity,
        args,
    } = expr
    else {
        return None;
    };
    let identity = constructor_identity.as_deref().unwrap_or(constructor);
    layouts
        .get(&(identity.to_owned(), args.len()))
        .map(|layout| layout.result)
}

/// Returns the checked semantic result type retained by a constructor layout.
pub(super) fn constructor_result_core_type(
    expr: &CoreExpr,
    layouts: &NativeConstructorLayouts,
) -> Option<CoreType> {
    let CoreExpr::ConstructorCall {
        constructor,
        constructor_identity,
        args,
    } = expr
    else {
        return None;
    };
    let identity = constructor_identity.as_deref().unwrap_or(constructor);
    layouts
        .get(&(identity.to_owned(), args.len()))?
        .result_core_type
        .clone()
}

/// Resolves and lowers one fixed named record construction.
pub(super) fn lower_record_construct(
    expr: &CoreExpr,
    layouts: &NativeConstructorLayouts,
    local_base: usize,
    lower_field: impl Fn(&CoreExpr, Option<&CoreType>) -> Result<(NativeExpr, NativeType), String>,
) -> Result<Option<(NativeExpr, NativeType)>, String> {
    let CoreExpr::RecordConstruct { name, fields } = expr else {
        return Ok(None);
    };
    let layout = record_layout(name, fields.len(), layouts)?;
    let mut source_names = HashSet::new();
    let mut lowered = Vec::with_capacity(fields.len());
    for field in fields {
        if !source_names.insert(field.key.as_str()) {
            return Err(format!(
                "error[native_ir.record_field_duplicate]: record `{name}` repeats field `{}`",
                field.key
            ));
        }
        let expected_core = layout
            .descriptor
            .fields()
            .iter()
            .position(|expected| expected.name() == Some(field.key.as_str()))
            .and_then(|index| layout.parameter_core_types.get(index))
            .and_then(Option::as_ref);
        let (value, ty) = lower_field(&field.value, expected_core)?;
        let value = rebase_callee_locals(&value, local_base, lowered.len());
        lowered.push((field, (value, ty)));
    }
    let ordered = ordered_record_fields(name, &layout.descriptor, fields, &lowered, local_base)?;
    let construct = NativeExpr::Construct {
        descriptor: layout.descriptor.clone(),
        encoded_layout: layout.encoded_layout.clone(),
        fields: ordered,
    };
    Ok(Some((
        if lowered.is_empty() {
            construct
        } else {
            NativeExpr::Let {
                bindings: lowered.into_iter().map(|(_, (value, _))| value).collect(),
                body: Box::new(construct),
            }
        },
        layout.result,
    )))
}

/// Returns the exact result kind for one named record construction.
pub(super) fn record_construct_result_type(
    expr: &CoreExpr,
    layouts: &NativeConstructorLayouts,
) -> Option<NativeType> {
    let CoreExpr::RecordConstruct { name, fields } = expr else {
        return None;
    };
    record_layout(name, fields.len(), layouts)
        .ok()
        .map(|layout| layout.result)
}

/// Resolves and lowers one persistent update of a fixed named record.
pub(super) fn lower_record_update(
    expr: &CoreExpr,
    layouts: &NativeConstructorLayouts,
    local_base: usize,
    lower_value: impl Fn(&CoreExpr) -> Result<(NativeExpr, NativeType), String>,
) -> Result<Option<(NativeExpr, NativeType)>, String> {
    let CoreExpr::RecordUpdate { base, name, fields } = expr else {
        return Ok(None);
    };
    let (base, base_type) = lower_value(base)?;
    let layout = record_update_layout(name, base_type, layouts)?;
    let mut source_names = HashSet::new();
    let mut lowered = Vec::with_capacity(fields.len());
    for field in fields {
        if !source_names.insert(field.key.as_str()) {
            return Err(format!(
                "error[native_ir.record_update_duplicate]: record `{name}` repeats update field `{}`",
                field.key
            ));
        }
        if !layout
            .descriptor
            .fields()
            .iter()
            .any(|expected| expected.name() == Some(&field.key))
        {
            return Err(format!(
                "error[native_ir.record_update_missing]: record `{name}` has no field `{}`",
                field.key
            ));
        }
        let (value, ty) = lower_value(&field.value)?;
        let value = rebase_callee_locals(&value, local_base, lowered.len().saturating_add(1));
        lowered.push((field, (value, ty)));
    }
    let NativeType::ManagedRef(semantic) = base_type else {
        return Err(format!(
            "error[native_ir.record_update_base]: record `{name}` requires a managed aggregate"
        ));
    };
    let fields = layout
        .descriptor
        .fields()
        .iter()
        .enumerate()
        .map(|(index, expected)| {
            let field = expected.name().ok_or_else(|| {
                format!("error[native_ir.record_update_shape]: record `{name}` has an unnamed field")
            })?;
            if let Some((source_index, (_, (_, actual)))) = lowered
                .iter()
                .enumerate()
                .find(|(_, (source, _))| source.key == field)
            {
                let expected = native_field_type(expected.field_type())?;
                if *actual != expected {
                    return Err(format!(
                        "error[native_ir.record_update_type]: record `{name}` field `{field}` requires {expected:?}, found {actual:?}"
                    ));
                }
                return Ok(NativeExpr::Param(
                    local_base.saturating_add(1).saturating_add(source_index),
                ));
            }
            let encoded = encode_aggregate_field_operation(semantic, index)
                .map_err(|error| format!("error[native_ir.record_update_operation]: {error}"))?;
            Ok(NativeExpr::ManagedOperation {
                encoded: Arc::from(encoded),
                args: vec![NativeExpr::Param(local_base)],
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut bindings = Vec::with_capacity(lowered.len().saturating_add(1));
    bindings.push(base);
    bindings.extend(lowered.into_iter().map(|(_, (value, _))| value));
    Ok(Some((
        NativeExpr::Let {
            bindings,
            body: Box::new(NativeExpr::Construct {
                descriptor: layout.descriptor.clone(),
                encoded_layout: layout.encoded_layout.clone(),
                fields,
            }),
        },
        layout.result,
    )))
}

/// Returns the exact result kind for one persistent named-record update.
pub(super) fn record_update_result_type(
    name: &str,
    base: NativeType,
    layouts: &NativeConstructorLayouts,
) -> Result<NativeType, String> {
    record_update_layout(name, base, layouts).map(|layout| layout.result)
}

/// Resolves the single physical layout selected by an explicit record update.
fn record_update_layout<'a>(
    name: &str,
    base: NativeType,
    layouts: &'a NativeConstructorLayouts,
) -> Result<&'a NativeConstructorLayout, String> {
    let mut candidates = layouts
        .iter()
        .filter(|((identity, _), layout)| identity == name && layout.result == base)
        .map(|(_, layout)| layout);
    let candidate = candidates.next().ok_or_else(|| {
        format!(
            "error[native_ir.record_update_identity]: `{name}` does not identify the receiver layout"
        )
    })?;
    if candidates.any(|layout| layout.encoded_layout != candidate.encoded_layout) {
        return Err(format!(
            "error[native_ir.record_update_layout]: record `{name}` has ambiguous physical layouts"
        ));
    }
    Ok(candidate)
}

/// Resolves one unique local or qualified record constructor layout.
fn record_layout<'a>(
    name: &str,
    arity: usize,
    layouts: &'a NativeConstructorLayouts,
) -> Result<&'a NativeConstructorLayout, String> {
    if let Some(layout) = layouts.get(&(name.to_string(), arity)) {
        return Ok(layout);
    }
    let mut candidates = layouts
        .iter()
        .filter(|((identity, candidate_arity), _)| {
            *candidate_arity == arity && identity.rsplit('.').next() == Some(name)
        })
        .map(|(_, layout)| layout);
    let candidate = candidates.next().ok_or_else(|| {
        format!("error[native_ir.record_layout]: record `{name}/{arity}` has no admitted layout")
    })?;
    if candidates.any(|layout| layout.encoded_layout != candidate.encoded_layout) {
        return Err(format!(
            "error[native_ir.record_layout]: record `{name}/{arity}` is ambiguous"
        ));
    }
    Ok(candidate)
}

/// Orders compiler locals into canonical physical fields after source-order evaluation.
fn ordered_record_fields(
    name: &str,
    descriptor: &ManagedAggregateDescriptor,
    source: &[CoreRecordExprField],
    lowered: &[(&CoreRecordExprField, (NativeExpr, NativeType))],
    local_base: usize,
) -> Result<Vec<NativeExpr>, String> {
    if descriptor.fields().len() != source.len() {
        return Err(format!(
            "error[native_ir.record_shape]: record `{name}` requires {} fields, found {}",
            descriptor.fields().len(),
            source.len()
        ));
    }
    descriptor
        .fields()
        .iter()
        .map(|expected| {
            let field = expected.name().ok_or_else(|| {
                format!("error[native_ir.record_shape]: record `{name}` has an unnamed field")
            })?;
            let (source_index, (_, (_, actual))) = lowered
                .iter()
                .enumerate()
                .find(|(_, (source, _))| source.key == field)
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.record_field_missing]: record `{name}` requires field `{field}`"
                    )
                })?;
            let expected = native_field_type(expected.field_type())?;
            if *actual != expected {
                return Err(format!(
                    "error[native_ir.record_field_type]: record `{name}` field `{field}` requires {expected:?}, found {actual:?}"
                ));
            }
            Ok(NativeExpr::Param(local_base.saturating_add(source_index)))
        })
        .collect()
}

/// Resolves one named field projection across every admitted physical variant.
pub(super) fn managed_field_projection(
    base: NativeType,
    record_name: Option<&str>,
    field: &str,
    layouts: &NativeConstructorLayouts,
) -> Result<(Arc<[u8]>, NativeType), String> {
    let NativeType::ManagedRef(semantic) = base else {
        return Err(format!(
            "error[native_ir.field_base]: field `{field}` requires a managed aggregate"
        ));
    };
    if let Some(record_name) = record_name {
        let identifies_receiver = layouts
            .iter()
            .any(|((identity, _), layout)| identity == record_name && layout.result == base);
        if !identifies_receiver {
            return Err(format!(
                "error[native_ir.record_identity]: `{record_name}` does not identify the receiver type"
            ));
        }
    }
    let mut seen = HashSet::<Arc<[u8]>>::new();
    let mut projection = None;
    let mut found_layout = false;
    for layout in layouts.values().filter(|layout| layout.result == base) {
        if !seen.insert(layout.encoded_layout.clone()) {
            continue;
        }
        found_layout = true;
        let (index, descriptor) = layout
            .descriptor
            .fields()
            .iter()
            .enumerate()
            .find(|(_, descriptor)| descriptor.name() == Some(field))
            .ok_or_else(|| {
                format!(
                    "error[native_ir.field_missing]: field `{field}` is not present in every `{}` layout",
                    layout.descriptor.canonical_type()
                )
            })?;
        let field_type = native_field_type(descriptor.field_type())?;
        match projection {
            Some(expected) if expected != (index, field_type) => {
                return Err(format!(
                    "error[native_ir.field_ambiguous]: field `{field}` has incompatible physical layouts"
                ));
            }
            None => projection = Some((index, field_type)),
            _ => {}
        }
    }
    if !found_layout {
        return Err(format!(
            "error[native_ir.field_layout]: managed field `{field}` has no admitted layout"
        ));
    }
    let (index, field_type) = projection.ok_or_else(|| {
        format!("error[native_ir.field_missing]: managed field `{field}` has no physical slot")
    })?;
    let encoded = if field_type.is_managed_reference() {
        encode_aggregate_field_operation(semantic, index)
    } else {
        encode_aggregate_scalar_field_operation(semantic, index)
    }
    .map_err(|error| format!("error[native_ir.field_operation]: {error}"))?;
    Ok((Arc::from(encoded), field_type))
}
