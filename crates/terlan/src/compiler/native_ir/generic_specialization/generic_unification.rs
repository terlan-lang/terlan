//! Structural unification and substitution for generic specialization.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreMapTypeField, CoreStructTypeField, CoreTupleTypeElem, CoreType};

pub(super) fn unify(
    template: &CoreType,
    concrete: &CoreType,
    generic_params: &[String],
    substitution: &mut HashMap<String, CoreType>,
) -> Result<(), String> {
    if let CoreType::Named(name) = template {
        if generic_params.iter().any(|parameter| parameter == name) {
            if concrete == template {
                return Ok(());
            }
            return match substitution.get(name) {
                Some(previous) if previous != concrete => Err(format!(
                    "error[native_ir.generic_unification]: `{name}` has incompatible concrete types `{}` and `{}`",
                    previous.contract_text(),
                    concrete.contract_text()
                )),
                Some(_) => Ok(()),
                None => {
                    substitution.insert(name.clone(), concrete.clone());
                    Ok(())
                }
            };
        }
    }
    match (template, concrete) {
        (
            CoreType::Apply {
                constructor: a,
                args: x,
            },
            CoreType::Apply {
                constructor: b,
                args: y,
            },
        ) if a.rsplit('.').next() == b.rsplit('.').next() && x.len() == y.len() => {
            for (left, right) in x.iter().zip(y) {
                unify(left, right, generic_params, substitution)?;
            }
            Ok(())
        }
        (CoreType::List(left), CoreType::List(right)) => {
            unify(left, right, generic_params, substitution)
        }
        (CoreType::Apply { constructor, args }, CoreType::List(element))
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            unify(&args[0], element, generic_params, substitution)
        }
        (CoreType::List(element), CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            unify(element, &args[0], generic_params, substitution)
        }
        (
            CoreType::Arrow {
                params: left_params,
                return_type: left_return,
            },
            CoreType::Arrow {
                params: right_params,
                return_type: right_return,
            },
        ) if left_params.len() == right_params.len() => {
            for (left, right) in left_params.iter().zip(right_params) {
                unify(left, right, generic_params, substitution)?;
            }
            unify(left_return, right_return, generic_params, substitution)
        }
        (CoreType::Tuple(left), CoreType::Tuple(right)) if left.len() == right.len() => {
            for (left, right) in left.iter().zip(right) {
                let left = match left {
                    CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
                };
                let right = match right {
                    CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
                };
                unify(left, right, generic_params, substitution)?;
            }
            Ok(())
        }
        (CoreType::Union(left), CoreType::Union(right)) if left.len() == right.len() => {
            for (left, right) in left.iter().zip(right) {
                unify(left, right, generic_params, substitution)?;
            }
            Ok(())
        }
        (CoreType::Union(variants), concrete) => {
            let mut best = None;
            for (index, variant) in variants.iter().enumerate() {
                let mut trial = substitution.clone();
                if unify(variant, concrete, generic_params, &mut trial).is_ok() {
                    let added = trial.len().saturating_sub(substitution.len());
                    if best.as_ref().is_none_or(|(best_added, best_index, _)| {
                        (added, index) < (*best_added, *best_index)
                    }) {
                        best = Some((added, index, trial));
                    }
                }
            }
            let Some((_, _, selected)) = best else {
                return Err(format!(
                    "error[native_ir.generic_unification]: `{}` does not contain `{}`",
                    template.contract_text(),
                    concrete.contract_text()
                ));
            };
            *substitution = selected;
            Ok(())
        }
        (CoreType::Map(left), CoreType::Map(right)) if left.len() == right.len() => {
            for (left, right) in left.iter().zip(right) {
                if left.key != right.key || left.operator != right.operator {
                    return Err(format!(
                        "error[native_ir.generic_unification]: map field `{}` does not match `{}`",
                        left.key, right.key
                    ));
                }
                unify(&left.value, &right.value, generic_params, substitution)?;
            }
            Ok(())
        }
        (
            CoreType::Struct {
                name: left_name,
                fields: left,
            },
            CoreType::Struct {
                name: right_name,
                fields: right,
            },
        ) if left_name == right_name && left.len() == right.len() => {
            for (left, right) in left.iter().zip(right) {
                if left.name != right.name || left.is_private != right.is_private {
                    return Err(format!(
                        "error[native_ir.generic_unification]: struct field `{}` does not match `{}`",
                        left.name, right.name
                    ));
                }
                unify(&left.ty, &right.ty, generic_params, substitution)?;
            }
            Ok(())
        }
        (CoreType::Struct { name, .. }, CoreType::Named(concrete))
        | (CoreType::Named(concrete), CoreType::Struct { name, .. })
            if name == concrete =>
        {
            Ok(())
        }
        _ if template == concrete => Ok(()),
        _ => Err(format!(
            "error[native_ir.generic_unification]: `{}` does not match `{}`",
            template.contract_text(),
            concrete.contract_text()
        )),
    }
}

pub(super) fn substitute(
    ty: &CoreType,
    generic_params: &[String],
    values: &HashMap<String, CoreType>,
) -> CoreType {
    match ty {
        CoreType::Named(name) if generic_params.iter().any(|parameter| parameter == name) => {
            values.get(name).cloned().unwrap_or_else(|| ty.clone())
        }
        CoreType::Apply { constructor, args } => CoreType::Apply {
            constructor: constructor.clone(),
            args: args
                .iter()
                .map(|ty| substitute(ty, generic_params, values))
                .collect(),
        },
        CoreType::List(element) => {
            CoreType::List(Box::new(substitute(element, generic_params, values)))
        }
        CoreType::Tuple(elements) => CoreType::Tuple(
            elements
                .iter()
                .map(|element| match element {
                    CoreTupleTypeElem::Type(ty) => {
                        CoreTupleTypeElem::Type(substitute(ty, generic_params, values))
                    }
                    CoreTupleTypeElem::Field { name, ty } => CoreTupleTypeElem::Field {
                        name: name.clone(),
                        ty: substitute(ty, generic_params, values),
                    },
                })
                .collect(),
        ),
        CoreType::Struct { name, fields } => CoreType::Struct {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|field| CoreStructTypeField {
                    name: field.name.clone(),
                    ty: substitute(&field.ty, generic_params, values),
                    is_private: field.is_private,
                })
                .collect(),
        },
        CoreType::Map(fields) => CoreType::Map(
            fields
                .iter()
                .map(|field| CoreMapTypeField {
                    key: field.key.clone(),
                    operator: field.operator.clone(),
                    value: substitute(&field.value, generic_params, values),
                })
                .collect(),
        ),
        CoreType::Arrow {
            params,
            return_type,
        } => CoreType::Arrow {
            params: params
                .iter()
                .map(|ty| substitute(ty, generic_params, values))
                .collect(),
            return_type: Box::new(substitute(return_type, generic_params, values)),
        },
        CoreType::Union(items) => CoreType::Union(
            items
                .iter()
                .map(|ty| substitute(ty, generic_params, values))
                .collect(),
        ),
        _ => ty.clone(),
    }
}
