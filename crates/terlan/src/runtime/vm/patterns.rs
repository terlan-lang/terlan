use std::collections::HashMap;

use crate::terlan_typeck::CorePattern;

use super::{core_pattern_kind, to_atom_payload, ReplValue};

/// Attempts to bind a pattern for case matching.
pub(super) fn bind_case_pattern(
    pattern: &CorePattern,
    value: ReplValue,
    env: &mut HashMap<String, ReplValue>,
) -> Result<bool, String> {
    let mut candidate = env.clone();
    match bind_repl_pattern(pattern, value, &mut candidate) {
        Ok(()) => {
            *env = candidate;
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

/// Binds one supported CoreIR pattern for REPL evaluation.
///
/// Inputs:
/// - `pattern`: CoreIR pattern from a let binding, function parameter, or
///   lambda parameter.
/// - `value`: evaluated value to match against the pattern.
/// - `env`: lexical environment to extend with bound variables.
///
/// Output:
/// - Success when the pattern matches and all variable bindings are inserted.
/// - Stable mismatch or unsupported-pattern errors otherwise.
///
/// Transformation:
/// - Applies the same structural pattern model used by Terlan source syntax to
///   compiler-owned REPL values without relying on any backend runtime.
pub(super) fn bind_repl_pattern(
    pattern: &CorePattern,
    value: ReplValue,
    env: &mut HashMap<String, ReplValue>,
) -> Result<(), String> {
    match pattern {
        CorePattern::Var(name) => {
            env.insert(name.clone(), value);
            Ok(())
        }
        CorePattern::Wildcard => Ok(()),
        CorePattern::Int(expected) => match value {
            ReplValue::Int(actual) if actual == *expected => Ok(()),
            other => Err(pattern_mismatch(pattern, &other)),
        },
        CorePattern::Float(expected) => match value {
            ReplValue::Float(actual) if actual == *expected => Ok(()),
            other => Err(pattern_mismatch(pattern, &other)),
        },
        CorePattern::Atom(expected) => match value {
            ReplValue::Atom(actual) if actual == *expected => Ok(()),
            ReplValue::Unit if expected == "Unit" => Ok(()),
            ReplValue::Bool(true) if expected == "true" => Ok(()),
            ReplValue::Bool(false) if expected == "false" => Ok(()),
            other => Err(pattern_mismatch(pattern, &other)),
        },
        CorePattern::Tuple(patterns) => match value {
            ReplValue::Tuple(values) if values.len() == patterns.len() => {
                bind_repl_patterns(patterns, values, env)
            }
            other => Err(pattern_mismatch(pattern, &other)),
        },
        CorePattern::List(patterns) => match value {
            ReplValue::List(values) if values.len() == patterns.len() => {
                bind_repl_patterns(patterns, values, env)
            }
            other => Err(pattern_mismatch(pattern, &other)),
        },
        CorePattern::ListCons { head, tail } => match value {
            ReplValue::List(values) if !values.is_empty() => {
                let mut values = values.into_iter();
                let first = values
                    .next()
                    .expect("non-empty list checked immediately above");
                bind_repl_pattern(head, first, env)?;
                bind_repl_pattern(tail, ReplValue::List(values.collect()), env)
            }
            other => Err(pattern_mismatch(pattern, &other)),
        },
        CorePattern::Constructor { name, args, .. } => {
            bind_constructor_pattern(name, args, value, env)
        }
        other => Err(format!(
            "REPL evaluator does not yet support pattern {}",
            core_pattern_kind(other)
        )),
    }
}

/// Binds a constructor-style CoreIR pattern.
fn bind_constructor_pattern(
    name: &str,
    args: &[CorePattern],
    value: ReplValue,
    env: &mut HashMap<String, ReplValue>,
) -> Result<(), String> {
    match (name, args, value) {
        ("Some", [pattern], ReplValue::Tuple(items)) => {
            bind_tagged_tuple_pattern("some", pattern, items, env)
        }
        ("Ok", [pattern], ReplValue::Tuple(items)) => {
            bind_tagged_tuple_pattern("ok", pattern, items, env)
        }
        ("Err", [pattern], ReplValue::Tuple(items)) => {
            bind_tagged_tuple_pattern("error", pattern, items, env)
        }
        ("None", [], ReplValue::Atom(actual)) if actual == "none" => Ok(()),
        (name, [], ReplValue::Atom(actual)) if actual == to_atom_payload(name) => Ok(()),
        (_, _, other) => Err(pattern_mismatch(
            &CorePattern::Constructor {
                name: name.to_string(),
                constructor_identity: None,
                args: args.to_vec(),
            },
            &other,
        )),
    }
}

/// Binds a single-argument tagged tuple constructor pattern.
fn bind_tagged_tuple_pattern(
    tag: &str,
    pattern: &CorePattern,
    items: Vec<ReplValue>,
    env: &mut HashMap<String, ReplValue>,
) -> Result<(), String> {
    let [ReplValue::Atom(actual), value] = items.as_slice() else {
        return Err("tagged tuple pattern expects two tuple elements".to_string());
    };
    if actual != tag {
        return Err(format!("tagged tuple expected `{tag}`, found `{actual}`"));
    }
    bind_repl_pattern(pattern, value.clone(), env)
}

/// Binds parallel pattern/value lists for structural REPL patterns.
fn bind_repl_patterns(
    patterns: &[CorePattern],
    values: Vec<ReplValue>,
    env: &mut HashMap<String, ReplValue>,
) -> Result<(), String> {
    for (pattern, value) in patterns.iter().zip(values.into_iter()) {
        bind_repl_pattern(pattern, value, env)?;
    }
    Ok(())
}

/// Builds a stable REPL pattern mismatch diagnostic.
fn pattern_mismatch(pattern: &CorePattern, value: &ReplValue) -> String {
    format!(
        "REPL pattern {} did not match {}",
        core_pattern_kind(pattern),
        value.render()
    )
}
