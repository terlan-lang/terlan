//! Sequential type recovery for checked list-comprehension generators.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CoreListComprehensionGenerator, CoreTupleTypeElem, CoreType};

use super::{
    bind_pattern, list_element, map_elements, set_element, specialize_expr, FunctionTypes,
};

pub(super) fn specialize_comprehension(
    yielded: &mut CoreExpr,
    generators: &mut [CoreListComprehensionGenerator],
    guards: &mut [CoreExpr],
    lift: &Option<String>,
    variables: &HashMap<String, CoreType>,
    functions: &FunctionTypes,
    module: &str,
) -> Option<CoreType> {
    let mut scoped = variables.clone();
    for generator in generators {
        let source_type = specialize_expr(&mut generator.source, &scoped, functions, module)?;
        let element = comprehension_element_type(&source_type)?;
        if !matches!(generator.source, CoreExpr::Cast { .. }) {
            generator.source = CoreExpr::Cast {
                expr: Box::new(generator.source.clone()),
                target_type: source_type,
            };
        }
        bind_pattern(&generator.pattern, &element, &mut scoped);
    }
    for guard in guards {
        specialize_expr(guard, &scoped, functions, module);
    }
    let yielded_type = specialize_expr(yielded, &scoped, functions, module)?;
    if !matches!(yielded, CoreExpr::Cast { .. }) {
        *yielded = CoreExpr::Cast {
            expr: Box::new(yielded.clone()),
            target_type: yielded_type.clone(),
        };
    }
    let result = CoreType::List(Box::new(yielded_type));
    Some(match lift {
        Some(container) => CoreType::Apply {
            constructor: container.clone(),
            args: vec![result],
        },
        None => result,
    })
}

fn comprehension_element_type(source: &CoreType) -> Option<CoreType> {
    if let Some(element) = list_element(source).or_else(|| set_element(source)) {
        return Some(element.clone());
    }
    if let Some((key, value)) = map_elements(source) {
        return Some(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(key.clone()),
            CoreTupleTypeElem::Type(value.clone()),
        ]));
    }
    match source {
        CoreType::Named(name)
        | CoreType::Struct { name, .. }
        | CoreType::Apply {
            constructor: name, ..
        } if name.rsplit('.').next() == Some("Range") => Some(CoreType::Int),
        _ => None,
    }
}
