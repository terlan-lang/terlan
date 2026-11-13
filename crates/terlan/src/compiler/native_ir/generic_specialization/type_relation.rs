//! Structural specialization ordering for contextual generic types.

use crate::terlan_typeck::{CoreTupleTypeElem, CoreType};

pub(super) fn contextual_type_specializes(symbolic: &CoreType, concrete: &CoreType) -> bool {
    relation(symbolic, concrete) == Some(true)
}

fn relation(symbolic: &CoreType, concrete: &CoreType) -> Option<bool> {
    if symbolic == concrete {
        return Some(false);
    }
    match (symbolic, concrete) {
        (CoreType::Named(name), _)
            if name != "Unit"
                && !name.contains('.')
                && name.starts_with(|character: char| character.is_ascii_uppercase()) =>
        {
            Some(true)
        }
        (
            CoreType::Apply {
                constructor: symbolic_constructor,
                args: symbolic_args,
            },
            CoreType::Apply {
                constructor: concrete_constructor,
                args: concrete_args,
            },
        ) if symbolic_constructor == concrete_constructor => {
            sequence_relation(symbolic_args, concrete_args)
        }
        (CoreType::Union(symbolic), CoreType::Union(concrete)) => {
            sequence_relation(symbolic, concrete)
        }
        (CoreType::List(symbolic), CoreType::List(concrete)) => relation(symbolic, concrete),
        (CoreType::Tuple(symbolic), CoreType::Tuple(concrete))
            if symbolic.len() == concrete.len() =>
        {
            symbolic
                .iter()
                .zip(concrete)
                .try_fold(false, |changed, (symbolic, concrete)| {
                    relation(tuple_type(symbolic), tuple_type(concrete)).map(|item| changed || item)
                })
        }
        _ => None,
    }
}

fn tuple_type(element: &CoreTupleTypeElem) -> &CoreType {
    match element {
        CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
    }
}

fn sequence_relation(symbolic: &[CoreType], concrete: &[CoreType]) -> Option<bool> {
    if symbolic.len() != concrete.len() {
        return None;
    }
    symbolic
        .iter()
        .zip(concrete)
        .try_fold(false, |changed, (symbolic, concrete)| {
            relation(symbolic, concrete).map(|item| changed || item)
        })
}
