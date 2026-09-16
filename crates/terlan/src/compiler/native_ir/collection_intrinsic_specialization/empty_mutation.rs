//! Checked first-use inference for fresh persistent collection bindings.

use super::*;

#[cfg(test)]
#[path = "empty_mutation_test.rs"]
mod tests;

#[derive(Clone, Copy)]
enum Collection {
    List,
    Map,
    Set,
}

/// Finds a checked use of a fresh binding, respecting intervening lexical scope.
pub(super) fn infer_binding_use(
    binding: &CoreLetBinding,
    later: &[CoreLetBinding],
    body: &CoreExpr,
    variables: &HashMap<String, CoreType>,
    functions: &FunctionTypes,
    module: &str,
) -> Option<CoreType> {
    let CorePattern::Var(name) = &binding.pattern else {
        return None;
    };
    let inference = UseInference {
        name,
        collection: empty_collection(&binding.value),
        functions,
        module,
    };
    inference.sequence(later, body, variables)
}

fn empty_collection(expr: &CoreExpr) -> Option<Collection> {
    match expr {
        CoreExpr::List(items) if items.is_empty() => Some(Collection::List),
        CoreExpr::Intrinsic(call) => match call.id {
            CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListNew) => Some(Collection::List),
            CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapNew) => Some(Collection::Map),
            CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetNew) => Some(Collection::Set),
            _ => None,
        },
        CoreExpr::Cast { expr, target_type }
            if list_element(target_type).is_some()
                || map_elements(target_type).is_some()
                || set_element(target_type).is_some() =>
        {
            empty_collection(expr)
        }
        _ => None,
    }
}

struct UseInference<'a> {
    name: &'a str,
    collection: Option<Collection>,
    functions: &'a FunctionTypes,
    module: &'a str,
}

impl UseInference<'_> {
    fn sequence(
        &self,
        bindings: &[CoreLetBinding],
        body: &CoreExpr,
        variables: &HashMap<String, CoreType>,
    ) -> Option<CoreType> {
        let mut variables = variables.clone();
        for binding in bindings {
            if let Some(ty) = self.expression(&binding.value, &variables) {
                return Some(ty);
            }
            let names =
                super::super::super::expression::free_variable_analysis::pattern_bound_names(
                    &binding.pattern,
                );
            if names.iter().any(|name| name == self.name) {
                return None;
            }
            let ty = specialize_expr(
                &mut binding.value.clone(),
                &variables,
                self.functions,
                self.module,
            );
            for name in names {
                variables.remove(&name);
            }
            if let Some(ty) = ty {
                bind_pattern(&binding.pattern, &ty, &mut variables);
            }
        }
        self.expression(body, &variables)
    }

    fn expression(
        &self,
        expr: &CoreExpr,
        variables: &HashMap<String, CoreType>,
    ) -> Option<CoreType> {
        if let Some(ty) = expected_call_argument_type(self.name, expr, self.functions, self.module)
        {
            return Some(ty);
        }
        let (receiver, method, args) = match expr {
            CoreExpr::Let { bindings, body } => return self.sequence(bindings, body, variables),
            CoreExpr::Cast { expr, .. } => return self.expression(expr, variables),
            CoreExpr::MutableReceiverCall {
                receiver,
                method,
                args,
                ..
            } => (receiver.as_ref(), method.as_str(), args.as_slice()),
            CoreExpr::Intrinsic(call) => {
                let method = match call.id {
                    CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListPush) => "push",
                    CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapPut) => "put",
                    CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetAdd) => "add",
                    _ => return None,
                };
                let (receiver, args) = call.args.split_first()?;
                (receiver, method, args)
            }
            _ => return None,
        };
        if !matches!(receiver, CoreExpr::Var(name) if name == self.name) {
            return None;
        }
        let infer = |value: &CoreExpr| {
            specialize_expr(&mut value.clone(), variables, self.functions, self.module)
        };
        match (self.collection?, method, args) {
            (Collection::List, "push", [value]) => Some(CoreType::List(Box::new(infer(value)?))),
            (Collection::Map, "put", [key, value]) => Some(CoreType::Apply {
                constructor: "Map".to_string(),
                args: vec![infer(key)?, infer(value)?],
            }),
            (Collection::Set, "add", [value]) => Some(CoreType::Apply {
                constructor: "Set".to_string(),
                args: vec![infer(value)?],
            }),
            _ => None,
        }
    }
}
