//! Compiler-owned stack-safe tail recursion for the maintained JavaScript backend.

use std::collections::{BTreeMap, BTreeSet};

use crate::terlan_typeck::{CoreExpr, CoreFunction, CoreModule, CorePattern};

use super::core_lowering::{
    core_case_literal_pattern_condition_to_js, core_clause_patterns_match_function_params,
    core_expr_to_js, emit_core_function_body_to_js,
};
use super::direct_helpers::is_direct_oxc_js_identifier;
use super::direct_reachability::reachable_direct_function_names;

/// Emits reachable recursive components through explicit JavaScript loops.
///
/// This deliberately runs before direct Oxc expression emission: JavaScript
/// engines are not trusted to implement proper tail calls. Non-tail calls keep
/// calling the public/private wrappers and therefore remain observably normal
/// JavaScript calls.
pub(super) fn emit_stack_safe_tail_module(module: &CoreModule) -> Option<String> {
    let reachable = reachable_direct_function_names(module);
    let functions = module
        .functions
        .iter()
        .filter(|function| reachable.contains(&function.name))
        .collect::<Vec<_>>();
    let indexes = functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.name.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let edges = functions
        .iter()
        .map(|function| {
            let body = eligible_body(function)?;
            let mut targets = BTreeSet::new();
            collect_tail_targets(body, &indexes, &mut targets);
            Some(targets)
        })
        .collect::<Option<Vec<_>>>()?;
    let components = recursive_components(&edges);
    if components.is_empty() {
        return None;
    }

    let mut component_by_function = BTreeMap::new();
    for (component_index, component) in components.iter().enumerate() {
        for function_index in component {
            component_by_function.insert(*function_index, component_index);
        }
    }

    let mut output = String::from(
        "function __terlan_checked_div(left, right) {\n\
         \x20 if (right === 0) {\n\
         \x20   const error = new Error(\"Terlan integer division by zero\");\n\
         \x20   error.terlanCode = \"DIVISION_BY_ZERO\";\n\
         \x20   error.terlanStatus = 4;\n\
         \x20   throw error;\n\
         \x20 }\n\
         \x20 return Math.trunc(left / right);\n\
         }\n\n",
    );
    for (component_index, component) in components.iter().enumerate() {
        emit_component_driver(
            &mut output,
            component_index,
            component,
            &functions,
            &indexes,
        )?;
    }
    for (function_index, function) in functions.iter().enumerate() {
        let export = if function.public { "export " } else { "" };
        let params = function
            .params
            .iter()
            .map(|param| param.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!(
            "{export}function {}({params}) {{\n",
            function.name
        ));
        if let Some(component_index) = component_by_function.get(&function_index) {
            let tag = components[*component_index]
                .iter()
                .position(|member| *member == function_index)?;
            output.push_str(&format!(
                "  return __terlan_tail_component_{component_index}({tag}, [{params}]);\n"
            ));
        } else {
            output.push_str(&emit_core_function_body_to_js(function)?);
        }
        output.push_str("}\n\n");
    }
    Some(output)
}

fn eligible_body(function: &CoreFunction) -> Option<&CoreExpr> {
    if !is_direct_oxc_js_identifier(&function.name) {
        return None;
    }
    let [clause] = function.clauses.as_slice() else {
        return None;
    };
    if clause.guard.is_some()
        || !core_clause_patterns_match_function_params(function, &clause.core_patterns)
    {
        return None;
    }
    clause.body.core_expr.as_ref()
}

fn collect_tail_targets(
    expr: &CoreExpr,
    indexes: &BTreeMap<&str, usize>,
    targets: &mut BTreeSet<usize>,
) {
    match expr {
        CoreExpr::Call { function, .. } => {
            if let Some(index) = indexes.get(function.as_str()) {
                targets.insert(*index);
            }
        }
        CoreExpr::Let { body, .. } => collect_tail_targets(body, indexes, targets),
        CoreExpr::If { clauses } => {
            for clause in clauses {
                collect_tail_targets(&clause.body, indexes, targets);
            }
        }
        CoreExpr::Case { clauses, .. } => {
            for clause in clauses {
                collect_tail_targets(&clause.body, indexes, targets);
            }
        }
        CoreExpr::Try {
            of_clauses,
            catch_clauses,
            after_clause: None,
            ..
        } => {
            for clause in of_clauses.iter().chain(catch_clauses) {
                collect_tail_targets(&clause.body, indexes, targets);
            }
        }
        _ => {}
    }
}

fn recursive_components(edges: &[BTreeSet<usize>]) -> Vec<Vec<usize>> {
    let mut assigned = BTreeSet::new();
    let mut components = Vec::new();
    for start in 0..edges.len() {
        if assigned.contains(&start) {
            continue;
        }
        let component = (0..edges.len())
            .filter(|candidate| {
                can_reach(start, *candidate, edges) && can_reach(*candidate, start, edges)
            })
            .collect::<Vec<_>>();
        let recursive = component.len() > 1 || edges[start].contains(&start);
        if recursive {
            assigned.extend(component.iter().copied());
            components.push(component);
        }
    }
    components
}

fn can_reach(start: usize, target: usize, edges: &[BTreeSet<usize>]) -> bool {
    let mut seen = BTreeSet::new();
    let mut pending = edges[start].iter().copied().collect::<Vec<_>>();
    while let Some(next) = pending.pop() {
        if next == target {
            return true;
        }
        if seen.insert(next) {
            pending.extend(edges[next].iter().copied());
        }
    }
    false
}

fn emit_component_driver(
    output: &mut String,
    component_index: usize,
    component: &[usize],
    functions: &[&CoreFunction],
    indexes: &BTreeMap<&str, usize>,
) -> Option<()> {
    output.push_str(&format!(
        "function __terlan_tail_component_{component_index}(__tag, __args) {{\n  while (true) {{\n    switch (__tag) {{\n"
    ));
    let component_set = component.iter().copied().collect::<BTreeSet<_>>();
    for (tag, function_index) in component.iter().copied().enumerate() {
        let function = functions[function_index];
        let params = function
            .params
            .iter()
            .map(|param| param.name.as_str())
            .collect::<Vec<_>>();
        output.push_str(&format!(
            "      case {tag}: {{\n        const [{}] = __args;\n",
            params.join(", ")
        ));
        emit_terminal(
            output,
            eligible_body(function)?,
            indexes,
            &component_set,
            component,
            8,
        )?;
        output.push_str("      }\n");
    }
    output.push_str(
        "      default: throw new Error(\"invalid Terlan tail component tag\");\n    }\n  }\n}\n\n",
    );
    Some(())
}

fn emit_terminal(
    output: &mut String,
    expr: &CoreExpr,
    indexes: &BTreeMap<&str, usize>,
    component_set: &BTreeSet<usize>,
    component: &[usize],
    indent: usize,
) -> Option<()> {
    let padding = " ".repeat(indent);
    match expr {
        CoreExpr::Call { function, args }
            if indexes
                .get(function.as_str())
                .is_some_and(|target| component_set.contains(target)) =>
        {
            let target = *indexes.get(function.as_str())?;
            let tag = component.iter().position(|member| *member == target)?;
            let args = args
                .iter()
                .map(core_expr_to_js)
                .collect::<Option<Vec<_>>>()?
                .join(", ");
            output.push_str(&format!(
                "{padding}__args = [{args}];\n{padding}__tag = {tag};\n{padding}continue;\n"
            ));
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                let CorePattern::Var(name) = &binding.pattern else {
                    return None;
                };
                if !is_direct_oxc_js_identifier(name) {
                    return None;
                }
                output.push_str(&format!(
                    "{padding}const {name} = {};\n",
                    core_expr_to_js(&binding.value)?
                ));
            }
            emit_terminal(output, body, indexes, component_set, component, indent)?;
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                output.push_str(&format!(
                    "{padding}if ({}) {{\n",
                    core_expr_to_js(&clause.condition)?
                ));
                emit_terminal(
                    output,
                    &clause.body,
                    indexes,
                    component_set,
                    component,
                    indent + 2,
                )?;
                output.push_str(&format!("{padding}}}\n"));
            }
            output.push_str(&format!(
                "{padding}throw new Error(\"non-exhaustive Terlan if\");\n"
            ));
        }
        CoreExpr::Case { scrutinee, clauses } => {
            let scrutinee_name = format!("__terlan_case_{indent}");
            output.push_str(&format!(
                "{padding}const {scrutinee_name} = {};\n",
                core_expr_to_js(scrutinee)?
            ));
            for clause in clauses {
                let condition = match &clause.pattern {
                    CorePattern::Wildcard => "true".to_string(),
                    pattern => core_case_literal_pattern_condition_to_js(&scrutinee_name, pattern)?,
                };
                let condition = if let Some(guard) = &clause.guard {
                    format!("({condition}) && ({})", core_expr_to_js(guard)?)
                } else {
                    condition
                };
                output.push_str(&format!("{padding}if ({condition}) {{\n"));
                emit_terminal(
                    output,
                    &clause.body,
                    indexes,
                    component_set,
                    component,
                    indent + 2,
                )?;
                output.push_str(&format!("{padding}}}\n"));
            }
            output.push_str(&format!(
                "{padding}throw new Error(\"non-exhaustive Terlan case\");\n"
            ));
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } if operator == "div" => output.push_str(&format!(
            "{padding}return __terlan_checked_div({}, {});\n",
            core_expr_to_js(left)?,
            core_expr_to_js(right)?
        )),
        _ => output.push_str(&format!("{padding}return {};\n", core_expr_to_js(expr)?)),
    }
    Some(())
}
