use std::collections::HashMap;
use std::fs;

use crate::terlan_typeck::{
    CoreCaseClause, CoreExpr, CoreFunction, CoreIntrinsicId, CoreModule, CorePattern,
    CorePrimitiveIntrinsic, CoreRuntimeCapability,
};

mod value;

use value::{is_implicit_type_name, normalize_core_string};
pub(crate) use value::{type_of_value, ReplClosure, ReplValue};

/// In-process Rust VM for checked Terlan CoreIR modules.
///
/// Inputs:
/// - CoreIR modules produced by the formal compiler pipeline.
///
/// Output:
/// - Executed Terlan values and routed runtime effects.
///
/// Transformation:
/// - Stores loaded modules by Terlan module name and executes supported CoreIR
///   directly in Rust without invoking BEAM, Erlang source generation, or a
///   target-specific runtime process.
#[derive(Debug, Default)]
pub(crate) struct TerlanVm {
    modules: HashMap<String, CoreModule>,
}

impl TerlanVm {
    /// Creates an empty Rust VM instance.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - Empty VM ready to receive checked modules.
    ///
    /// Transformation:
    /// - Initializes the module table used by later execution calls.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Loads one checked CoreIR module into the VM.
    ///
    /// Inputs:
    /// - `module`: CoreIR module produced by the compiler frontend.
    ///
    /// Output:
    /// - Replaces any module with the same Terlan module name.
    ///
    /// Transformation:
    /// - Indexes the module by its source-facing module path so execution can
    ///   remain independent from backend artifact names.
    pub(crate) fn load_module(&mut self, module: CoreModule) {
        self.modules.insert(module.module.clone(), module);
    }

    /// Executes one public zero-arity function from a loaded module.
    ///
    /// Inputs:
    /// - `module_name`: Terlan module name to execute from.
    /// - `function_name`: zero-arity function entrypoint.
    /// - `output`: callback for console output effects.
    ///
    /// Output:
    /// - Evaluated VM value, or a stable VM error.
    ///
    /// Transformation:
    /// - Resolves the loaded module and delegates expression execution to the
    ///   CoreIR interpreter owned by this runtime module.
    pub(crate) fn execute_zero_arity(
        &self,
        module_name: &str,
        function_name: &str,
        output: &mut dyn FnMut(&str),
    ) -> Result<ReplValue, String> {
        let module = self
            .modules
            .get(module_name)
            .ok_or_else(|| format!("Terlan VM has not loaded module `{module_name}`"))?;
        evaluate_repl_function_with_output(module, function_name, output)
    }
}

/// Evaluates one public zero-arity function from a compiled CoreIR module.
///
/// Inputs:
/// - `core`: compiled module produced by the formal compiler pipeline.
/// - `function_name`: generated REPL entry function to evaluate.
///
/// Output:
/// - Renderable REPL value on success.
/// - Stable evaluator error text when the selected CoreIR form is unsupported.
///
/// Transformation:
/// - Finds the selected CoreIR function, evaluates the first clause body in an
///   empty environment, and dispatches local calls through the same module.
#[cfg(test)]
pub(crate) fn evaluate_repl_function(
    core: &CoreModule,
    function_name: &str,
) -> Result<ReplValue, String> {
    let mut output = |value: &str| println!("{value}");
    evaluate_repl_function_with_output(core, function_name, &mut output)
}

/// Evaluates one public zero-arity function with an explicit output sink.
///
/// Inputs:
/// - `core`: compiled module produced by the formal compiler pipeline.
/// - `function_name`: generated REPL entry function to evaluate.
/// - `output`: callback invoked for console output effects.
///
/// Output:
/// - Renderable REPL value on success.
/// - Stable evaluator error text when the selected CoreIR form is unsupported.
///
/// Transformation:
/// - Finds the selected CoreIR function and evaluates it while routing selected
///   effect hooks through the caller-owned output sink instead of directly
///   choosing text or structured REPL output.
pub(crate) fn evaluate_repl_function_with_output(
    core: &CoreModule,
    function_name: &str,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let function = find_function(core, function_name, 0)
        .ok_or_else(|| format!("missing REPL function {function_name}/0 in CoreIR"))?;
    let clause = function
        .clauses
        .first()
        .ok_or_else(|| format!("REPL function {function_name}/0 has no clauses"))?;
    let body =
        clause.body.core_expr.as_ref().ok_or_else(|| {
            format!("REPL function {function_name}/0 has no executable CoreIR body")
        })?;
    let mut env = HashMap::new();
    evaluate_expr(core, body, &mut env, output)
}

/// Evaluates one supported CoreIR expression.
///
/// Inputs:
/// - `core`: containing module used for local function calls.
/// - `expr`: CoreIR expression payload.
/// - `env`: mutable lexical environment for variables and let bindings.
/// - `output`: callback invoked for console output effects.
///
/// Output:
/// - Evaluated REPL value or unsupported-form error text.
///
/// Transformation:
/// - Recursively interprets the selected CoreIR subset directly, without
///   emitting target code or invoking a target runtime.
fn evaluate_expr(
    core: &CoreModule,
    expr: &CoreExpr,
    env: &mut HashMap<String, ReplValue>,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    match expr {
        CoreExpr::Int(value) => Ok(ReplValue::Int(*value)),
        CoreExpr::Float(value) => Ok(ReplValue::Float(value.clone())),
        CoreExpr::Binary(value) => Ok(ReplValue::String(normalize_core_string(value))),
        CoreExpr::Atom(value) if value == "Unit" => Ok(ReplValue::Unit),
        CoreExpr::Atom(value) if value == "true" => Ok(ReplValue::Bool(true)),
        CoreExpr::Atom(value) if value == "false" => Ok(ReplValue::Bool(false)),
        CoreExpr::Atom(value) => Ok(ReplValue::Atom(value.clone())),
        CoreExpr::Var(name) if name == "Unit" => Ok(ReplValue::Unit),
        CoreExpr::Var(name) if name == "true" => Ok(ReplValue::Bool(true)),
        CoreExpr::Var(name) if name == "false" => Ok(ReplValue::Bool(false)),
        CoreExpr::Var(name) if is_implicit_type_name(name) => Ok(ReplValue::Type(name.clone())),
        CoreExpr::Var(name) => env
            .get(name)
            .cloned()
            .or_else(|| constant_value(name))
            .ok_or_else(|| format!("unknown REPL variable `{name}`")),
        CoreExpr::Tuple(items) => evaluate_exprs(core, items, env, output).map(ReplValue::Tuple),
        CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            evaluate_exprs(core, items, env, output).map(ReplValue::List)
        }
        CoreExpr::Map(fields) => {
            let mut entries = Vec::new();
            for field in fields {
                let value = evaluate_expr(core, &field.value, env, output)?;
                entries.push((ReplValue::String(field.key.clone()), value));
            }
            Ok(ReplValue::Map(entries))
        }
        CoreExpr::ListCons { head, tail } => {
            let head = evaluate_expr(core, head, env, output)?;
            let tail = evaluate_expr(core, tail, env, output)?;
            match tail {
                ReplValue::List(mut items) => {
                    items.insert(0, head);
                    Ok(ReplValue::List(items))
                }
                other => Err(format!(
                    "list cons tail must evaluate to List, found {}",
                    other.render()
                )),
            }
        }
        CoreExpr::Let { bindings, body } => {
            let mut next_env = env.clone();
            for binding in bindings {
                let value = evaluate_expr(core, &binding.value, &mut next_env, output)?;
                bind_repl_pattern(&binding.pattern, value, &mut next_env)?;
            }
            evaluate_expr(core, body, &mut next_env, output)
        }
        CoreExpr::UnaryOp { operator, operand } => {
            let value = evaluate_expr(core, operand, env, output)?;
            evaluate_unary(operator, value)
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            let left = evaluate_expr(core, left, env, output)?;
            let right = evaluate_expr(core, right, env, output)?;
            evaluate_binary(operator, left, right)
        }
        CoreExpr::Lam { params, body } => Ok(ReplValue::Closure(ReplClosure {
            params: params.clone(),
            body: (**body).clone(),
            env: env.clone(),
        })),
        CoreExpr::Call { function, args } => evaluate_call(core, function, args, env, output),
        CoreExpr::FunctionCall { callee, args } => {
            evaluate_function_call(core, callee, args, env, output)
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => evaluate_remote_call(core, module, function, args, env, output),
        CoreExpr::ConstructorCall {
            constructor, args, ..
        } => evaluate_constructor_call(core, constructor, args, env, output),
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            ..
        } => evaluate_mutable_receiver_call(core, receiver, method, args, env, output),
        CoreExpr::Case { scrutinee, clauses } => {
            evaluate_case(core, scrutinee, clauses, env, output)
        }
        CoreExpr::If { clauses } => evaluate_if(core, clauses, env, output),
        CoreExpr::Intrinsic(call) => evaluate_intrinsic(core, call, env, output),
        other => Err(format!(
            "CoreIR evaluator does not yet support {}",
            core_expr_kind(other)
        )),
    }
}

/// Returns a compiler-known source constant value.
fn constant_value(name: &str) -> Option<ReplValue> {
    match name {
        "None" => Some(ReplValue::Atom("none".to_string())),
        "Lt" => Some(ReplValue::Atom("lt".to_string())),
        "Eq" => Some(ReplValue::Atom("eq".to_string())),
        "Gt" => Some(ReplValue::Atom("gt".to_string())),
        "Ok" => Some(ReplValue::Atom("ok".to_string())),
        "Err" => Some(ReplValue::Atom("error".to_string())),
        other if starts_with_uppercase(other) => Some(ReplValue::Atom(to_atom_payload(other))),
        _ => None,
    }
}

/// Returns whether a name starts with an uppercase character.
fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().map(char::is_uppercase).unwrap_or(false)
}

/// Converts a Terlan constant-like identifier into its atom payload.
fn to_atom_payload(name: &str) -> String {
    let mut payload = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if index > 0 {
                payload.push('_');
            }
            payload.extend(ch.to_lowercase());
        } else {
            payload.push(ch);
        }
    }
    payload
}

/// Evaluates a list of CoreIR expressions in order.
///
/// Inputs:
/// - `core`: containing module used for calls.
/// - `items`: ordered CoreIR expressions.
/// - `env`: lexical environment shared across the evaluation.
/// - `output`: callback invoked for console output effects.
///
/// Output:
/// - Ordered evaluated values, or the first evaluation error.
///
/// Transformation:
/// - Preserves source order and short-circuits on unsupported or invalid
///   subexpressions.
fn evaluate_exprs(
    core: &CoreModule,
    items: &[CoreExpr],
    env: &mut HashMap<String, ReplValue>,
    output: &mut dyn FnMut(&str),
) -> Result<Vec<ReplValue>, String> {
    items
        .iter()
        .map(|expr| evaluate_expr(core, expr, env, output))
        .collect()
}

/// Evaluates a local function call.
///
/// Inputs:
/// - `core`: containing module with callable functions.
/// - `function`: source function name.
/// - `args`: evaluated argument expressions.
/// - `env`: caller lexical environment.
/// - `output`: callback invoked for console output effects.
///
/// Output:
/// - Called function's evaluated return value.
///
/// Transformation:
/// - Resolves by name/arity, binds simple variable parameters from the first
///   matching clause, and evaluates the clause body in a fresh environment.
fn evaluate_call(
    core: &CoreModule,
    function: &str,
    args: &[CoreExpr],
    env: &mut HashMap<String, ReplValue>,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let evaluated_args = evaluate_exprs(core, args, env, output)?;
    match (function, evaluated_args.as_slice()) {
        ("type_of", [value]) => return Ok(ReplValue::Type(type_of_value(value))),
        ("is_type", [value, ReplValue::Type(expected)]) => {
            return Ok(ReplValue::Bool(type_of_value(value) == expected.as_str()));
        }
        ("is_type", [_, other]) => {
            return Err(format!(
                "is_type expects a Type value as its second argument, found {}",
                other.render()
            ));
        }
        ("type_of", _) => return Err("type_of expects one argument".to_string()),
        ("is_type", _) => return Err("is_type expects two arguments".to_string()),
        _ => {}
    }
    if function == "println" && evaluated_args.len() == 1 {
        return evaluate_console_println(&evaluated_args[0], output);
    }
    let function = find_function(core, function, evaluated_args.len()).ok_or_else(|| {
        format!(
            "unknown REPL function `{function}/{}`",
            evaluated_args.len()
        )
    })?;
    apply_core_function(core, function, evaluated_args, output)
}

/// Applies a CoreIR function to already evaluated arguments.
fn apply_core_function(
    core: &CoreModule,
    function: &CoreFunction,
    evaluated_args: Vec<ReplValue>,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let clause = function
        .clauses
        .first()
        .ok_or_else(|| format!("function `{}` has no clauses", function.name))?;
    let mut call_env = HashMap::new();
    for (index, value) in evaluated_args.into_iter().enumerate() {
        match clause.core_patterns.get(index).and_then(Option::as_ref) {
            Some(CorePattern::Var(name)) => {
                call_env.insert(name.clone(), value);
            }
            Some(CorePattern::Wildcard) => {}
            Some(pattern) => {
                return Err(format!(
                    "REPL evaluator does not yet support call pattern {}",
                    core_pattern_kind(pattern)
                ));
            }
            None => {
                let Some(param) = function.params.get(index) else {
                    return Err(format!(
                        "function `{}` has missing parameter metadata",
                        function.name
                    ));
                };
                call_env.insert(param.name.clone(), value);
            }
        }
    }
    let body = clause
        .body
        .core_expr
        .as_ref()
        .ok_or_else(|| format!("function `{}` has no executable CoreIR body", function.name))?;
    evaluate_expr(core, body, &mut call_env, output)
}

/// Evaluates a remote CoreIR call through loaded std-native dispatch.
fn evaluate_remote_call(
    core: &CoreModule,
    module: &str,
    function: &str,
    args: &[CoreExpr],
    env: &mut HashMap<String, ReplValue>,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let evaluated_args = evaluate_exprs(core, args, env, output)?;
    evaluate_std_remote(module, function, evaluated_args)
}

/// Evaluates a source constructor call into its runtime value shape.
fn evaluate_constructor_call(
    core: &CoreModule,
    constructor: &str,
    args: &[CoreExpr],
    env: &mut HashMap<String, ReplValue>,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let args = evaluate_exprs(core, args, env, output)?;
    match constructor {
        "Some" => unary_tagged_tuple("some", args),
        "Ok" => unary_tagged_tuple("ok", args),
        "Err" => unary_tagged_tuple("error", args),
        "List" => Ok(ReplValue::List(args)),
        "Set" => Ok(ReplValue::Set(unique_values(args))),
        "Map" | "Object" => map_from_entries(args),
        other if args.is_empty() => constant_value(other)
            .ok_or_else(|| format!("unsupported zero-arity constructor `{other}`")),
        other => {
            let mut tuple = Vec::with_capacity(args.len() + 1);
            tuple.push(ReplValue::Atom(to_atom_payload(other)));
            tuple.extend(args);
            Ok(ReplValue::Tuple(tuple))
        }
    }
}

/// Builds a one-value tagged tuple constructor.
fn unary_tagged_tuple(tag: &str, args: Vec<ReplValue>) -> Result<ReplValue, String> {
    let [value] = args.as_slice() else {
        return Err(format!("{tag} constructor expects one argument"));
    };
    Ok(ReplValue::Tuple(vec![
        ReplValue::Atom(tag.to_string()),
        value.clone(),
    ]))
}

/// Builds a map value from `{key, value}` tuple entries.
fn map_from_entries(entries: Vec<ReplValue>) -> Result<ReplValue, String> {
    let mut map = Vec::<(ReplValue, ReplValue)>::new();
    for entry in entries {
        let ReplValue::Tuple(items) = entry else {
            return Err(format!("Map entry expects tuple, found {}", entry.render()));
        };
        let [key, value] = items.as_slice() else {
            return Err("Map entry expects two tuple elements".to_string());
        };
        map_insert(&mut map, key.clone(), value.clone());
    }
    Ok(ReplValue::Map(map))
}

/// Evaluates and applies a mutable receiver call.
fn evaluate_mutable_receiver_call(
    core: &CoreModule,
    receiver: &CoreExpr,
    method: &str,
    args: &[CoreExpr],
    env: &mut HashMap<String, ReplValue>,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let mut receiver_value = evaluate_expr(core, receiver, env, output)?;
    let args = evaluate_exprs(core, args, env, output)?;
    apply_mutable_receiver(method, &mut receiver_value, args)?;
    if let CoreExpr::Var(name) = receiver {
        env.insert(name.clone(), receiver_value);
    }
    Ok(ReplValue::Unit)
}

/// Applies one compiler-known mutable receiver update.
fn apply_mutable_receiver(
    method: &str,
    receiver: &mut ReplValue,
    args: Vec<ReplValue>,
) -> Result<(), String> {
    match (receiver, method, args.as_slice()) {
        (ReplValue::List(items), "push", [value]) => {
            items.push(value.clone());
            Ok(())
        }
        (ReplValue::List(items), "clear", []) => {
            items.clear();
            Ok(())
        }
        (ReplValue::Map(entries), "put", [key, value]) => {
            map_insert(entries, key.clone(), value.clone());
            Ok(())
        }
        (ReplValue::Map(entries), "remove", [key]) => {
            entries.retain(|(entry_key, _)| entry_key != key);
            Ok(())
        }
        (ReplValue::Map(entries), "clear", []) => {
            entries.clear();
            Ok(())
        }
        (ReplValue::Set(items), "add", [value]) => {
            if !items.contains(value) {
                items.push(value.clone());
            }
            Ok(())
        }
        (ReplValue::Set(items), "remove", [value]) => {
            items.retain(|item| item != value);
            Ok(())
        }
        (ReplValue::Set(items), "clear", []) => {
            items.clear();
            Ok(())
        }
        (receiver, method, _) => Err(format!(
            "unsupported mutable receiver `{}` for {}",
            method,
            receiver.render()
        )),
    }
}

/// Inserts or replaces a key-value entry in insertion order.
fn map_insert(entries: &mut Vec<(ReplValue, ReplValue)>, key: ReplValue, value: ReplValue) {
    if let Some((_, existing)) = entries.iter_mut().find(|(entry_key, _)| *entry_key == key) {
        *existing = value;
    } else {
        entries.push((key, value));
    }
}

/// Evaluates a CoreIR case expression.
fn evaluate_case(
    core: &CoreModule,
    scrutinee: &CoreExpr,
    clauses: &[CoreCaseClause],
    env: &mut HashMap<String, ReplValue>,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let value = evaluate_expr(core, scrutinee, env, output)?;
    for clause in clauses {
        let mut branch_env = env.clone();
        if !bind_case_pattern(&clause.pattern, value.clone(), &mut branch_env)? {
            continue;
        }
        if let Some(guard) = &clause.guard {
            match evaluate_expr(core, guard, &mut branch_env, output)? {
                ReplValue::Bool(true) => {}
                ReplValue::Bool(false) => continue,
                other => {
                    return Err(format!("case guard expects Bool, found {}", other.render()));
                }
            }
        }
        return evaluate_expr(core, &clause.body, &mut branch_env, output);
    }
    Err(format!("no case clause matched {}", value.render()))
}

/// Evaluates a CoreIR if expression.
fn evaluate_if(
    core: &CoreModule,
    clauses: &[crate::terlan_typeck::CoreIfClause],
    env: &mut HashMap<String, ReplValue>,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    for clause in clauses {
        match evaluate_expr(core, &clause.condition, env, output)? {
            ReplValue::Bool(true) => return evaluate_expr(core, &clause.body, env, output),
            ReplValue::Bool(false) => {}
            other => {
                return Err(format!(
                    "if condition expects Bool, found {}",
                    other.render()
                ))
            }
        }
    }
    Err("no if clause matched".to_string())
}

/// Attempts to bind a pattern for case matching.
fn bind_case_pattern(
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

/// Evaluates a first-class function call in the REPL evaluator.
///
/// Inputs:
/// - `core`: containing module used for nested expression evaluation.
/// - `callee`: CoreIR expression expected to evaluate to a closure.
/// - `args`: call argument expressions.
/// - `env`: caller lexical environment.
/// - `output`: callback invoked for console output effects.
///
/// Output:
/// - Closure body result, or a stable evaluator error when the callee or
///   parameter patterns are unsupported.
///
/// Transformation:
/// - Evaluates the callee and arguments in caller order, then evaluates the
///   lambda body in the captured lexical environment extended with argument
///   bindings.
fn evaluate_function_call(
    core: &CoreModule,
    callee: &CoreExpr,
    args: &[CoreExpr],
    env: &mut HashMap<String, ReplValue>,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let callee = evaluate_expr(core, callee, env, output)?;
    let evaluated_args = evaluate_exprs(core, args, env, output)?;
    let ReplValue::Closure(closure) = callee else {
        return Err(format!(
            "function-value call expects Function, found {}",
            callee.render()
        ));
    };
    apply_closure(core, closure, evaluated_args, output)
}

/// Applies a captured REPL closure.
///
/// Inputs:
/// - `core`: containing module used for nested calls in the body.
/// - `closure`: captured lambda value.
/// - `args`: evaluated argument values.
/// - `output`: callback invoked for console output effects.
///
/// Output:
/// - Evaluated closure body or arity/pattern error.
///
/// Transformation:
/// - Starts from the closure's captured environment, binds simple CoreIR
///   parameter patterns, and evaluates the body in that extended environment.
fn apply_closure(
    core: &CoreModule,
    closure: ReplClosure,
    args: Vec<ReplValue>,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    if closure.params.len() != args.len() {
        return Err(format!(
            "function-value call expects {} argument(s), found {}",
            closure.params.len(),
            args.len()
        ));
    }
    let mut call_env = closure.env;
    for (pattern, value) in closure.params.iter().zip(args.into_iter()) {
        bind_repl_pattern(pattern, value, &mut call_env)?;
    }
    evaluate_expr(core, &closure.body, &mut call_env, output)
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
fn bind_repl_pattern(
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
///
/// Inputs:
/// - `patterns`: ordered structural subpatterns.
/// - `values`: ordered evaluated values with matching arity.
/// - `env`: lexical environment to extend.
///
/// Output:
/// - Success when every subpattern matches, or the first mismatch error.
///
/// Transformation:
/// - Zips already arity-checked aggregate patterns and values into recursive
///   binding calls so tuple and list destructuring share one implementation.
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
///
/// Inputs:
/// - `pattern`: pattern that failed to match.
/// - `value`: evaluated value that was checked.
///
/// Output:
/// - Human-readable mismatch text.
///
/// Transformation:
/// - Converts internal pattern kind and REPL value rendering into a compact
///   diagnostic suitable for text and JSON REPL modes.
fn pattern_mismatch(pattern: &CorePattern, value: &ReplValue) -> String {
    format!(
        "REPL pattern {} did not match {}",
        core_pattern_kind(pattern),
        value.render()
    )
}

/// Evaluates a supported CoreIR intrinsic call.
///
/// Inputs:
/// - `core`: containing module used for nested evaluation.
/// - `call`: CoreIR intrinsic payload.
/// - `env`: lexical environment.
/// - `output`: callback invoked for console output effects.
///
/// Output:
/// - Intrinsic result value or unsupported-intrinsic error.
///
/// Transformation:
/// - Implements selected target-neutral primitive operations and the first REPL
///   std effect hook directly in the compiler-owned evaluator.
fn evaluate_intrinsic(
    core: &CoreModule,
    call: &crate::terlan_typeck::CoreIntrinsicCall,
    env: &mut HashMap<String, ReplValue>,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let args = evaluate_exprs(core, &call.args, env, output)?;
    match &call.id {
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TypeOf) => {
            let [value] = args.as_slice() else {
                return Err("core.type.type_of expects one argument".to_string());
            };
            Ok(ReplValue::Type(type_of_value(value)))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IsType) => {
            let [value, ReplValue::Type(expected)] = args.as_slice() else {
                return Err("core.type.is_type expects value and Type arguments".to_string());
            };
            Ok(ReplValue::Bool(type_of_value(value) == expected.as_str()))
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::ConsolePrintln) => {
            let [value] = args.as_slice() else {
                return Err("runtime.console.println expects one argument".to_string());
            };
            evaluate_console_println(value, output)
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileExists) => {
            let [ReplValue::String(path)] = args.as_slice() else {
                return Err("runtime.file.exists expects String path".to_string());
            };
            Ok(ReplValue::Bool(
                fs::metadata(path)
                    .map(|meta| meta.is_file())
                    .unwrap_or(false),
            ))
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileReadText) => {
            let [ReplValue::String(path)] = args.as_slice() else {
                return Err("runtime.file.read_text expects String path".to_string());
            };
            Ok(match fs::read_to_string(path) {
                Ok(contents) => ok_value(ReplValue::String(contents)),
                Err(err) => file_error_value(path, &err),
            })
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileWriteText) => {
            let [ReplValue::String(path), ReplValue::String(contents)] = args.as_slice() else {
                return Err("runtime.file.write_text expects String path and content".to_string());
            };
            Ok(match fs::write(path, contents) {
                Ok(()) => ok_value(ReplValue::Unit),
                Err(err) => file_error_value(path, &err),
            })
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileAppendText) => {
            let [ReplValue::String(path), ReplValue::String(contents)] = args.as_slice() else {
                return Err("runtime.file.append_text expects String path and content".to_string());
            };
            let result = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut file| {
                    use std::io::Write;
                    file.write_all(contents.as_bytes())
                });
            Ok(match result {
                Ok(()) => ok_value(ReplValue::Unit),
                Err(err) => file_error_value(path, &err),
            })
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileDelete) => {
            let [ReplValue::String(path)] = args.as_slice() else {
                return Err("runtime.file.delete expects String path".to_string());
            };
            Ok(match fs::remove_file(path) {
                Ok(()) => ok_value(ReplValue::Unit),
                Err(err) => file_error_value(path, &err),
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolEqual) => {
            let [left, right] = args.as_slice() else {
                return Err("core.bool.equal expects two arguments".to_string());
            };
            Ok(ReplValue::Bool(left == right))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolCompare) => {
            compare_bool(args.as_slice())
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IntToString) => {
            let [value] = args.as_slice() else {
                return Err("core.int.to_string expects one argument".to_string());
            };
            match value {
                ReplValue::Int(value) => Ok(ReplValue::String(value.to_string())),
                other => Err(format!(
                    "core.int.to_string expects Int, found {}",
                    other.render()
                )),
            }
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IntFromString) => {
            let [ReplValue::String(value)] = args.as_slice() else {
                return Err("core.int.from_string expects String".to_string());
            };
            Ok(value
                .parse::<i64>()
                .map(|value| some_value(ReplValue::Int(value)))
                .unwrap_or_else(|_| none_value()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::FloatToString) => {
            let [ReplValue::Float(value)] = args.as_slice() else {
                return Err("core.float.to_string expects Float".to_string());
            };
            Ok(ReplValue::String(value.clone()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::FloatFromString) => {
            let [ReplValue::String(value)] = args.as_slice() else {
                return Err("core.float.from_string expects String".to_string());
            };
            Ok(value
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|_| some_value(ReplValue::Float(value.clone())))
                .unwrap_or_else(none_value))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::AtomToString) => {
            let [ReplValue::Atom(value)] = args.as_slice() else {
                return Err("core.atom.to_string expects Atom".to_string());
            };
            Ok(ReplValue::String(value.clone()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringEqual) => {
            let [left, right] = args.as_slice() else {
                return Err("core.string.equal expects two arguments".to_string());
            };
            Ok(ReplValue::Bool(left == right))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringCompare) => {
            compare_string(args.as_slice())
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringToString) => {
            let [value] = args.as_slice() else {
                return Err("core.string.to_string expects one argument".to_string());
            };
            match value {
                ReplValue::String(value) => Ok(ReplValue::String(value.clone())),
                other => Err(format!(
                    "core.string.to_string expects String, found {}",
                    other.render()
                )),
            }
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringFromString) => {
            let [ReplValue::String(value)] = args.as_slice() else {
                return Err("core.string.from_string expects String".to_string());
            };
            Ok(some_value(ReplValue::String(value.clone())))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringIsEmpty) => {
            string_predicate(args.as_slice(), "core.string.is_empty", |value| {
                value.is_empty()
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringAppend) => {
            let [ReplValue::String(left), ReplValue::String(right)] = args.as_slice() else {
                return Err("core.string.append expects two Strings".to_string());
            };
            Ok(ReplValue::String(format!("{left}{right}")))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringConcat) => {
            let [ReplValue::List(values)] = args.as_slice() else {
                return Err("core.string.concat expects List[String]".to_string());
            };
            let mut result = String::new();
            for value in values {
                let ReplValue::String(value) = value else {
                    return Err(format!(
                        "core.string.concat expects String item, found {}",
                        value.render()
                    ));
                };
                result.push_str(value);
            }
            Ok(ReplValue::String(result))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringContains) => {
            string_binary_predicate(args.as_slice(), "core.string.contains", |value, pattern| {
                value.contains(pattern)
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringStartsWith) => {
            string_binary_predicate(
                args.as_slice(),
                "core.string.starts_with",
                |value, pattern| value.starts_with(pattern),
            )
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringEndsWith) => {
            string_binary_predicate(
                args.as_slice(),
                "core.string.ends_with",
                |value, pattern| value.ends_with(pattern),
            )
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolToString) => {
            let [value] = args.as_slice() else {
                return Err("core.bool.to_string expects one argument".to_string());
            };
            match value {
                ReplValue::Bool(value) => Ok(ReplValue::String(value.to_string())),
                other => Err(format!(
                    "core.bool.to_string expects Bool, found {}",
                    other.render()
                )),
            }
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolFromString) => {
            let [ReplValue::String(value)] = args.as_slice() else {
                return Err("core.bool.from_string expects String".to_string());
            };
            Ok(match value.as_str() {
                "true" => some_value(ReplValue::Bool(true)),
                "false" => some_value(ReplValue::Bool(false)),
                _ => none_value(),
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringLength) => {
            let [value] = args.as_slice() else {
                return Err("core.string.length expects one argument".to_string());
            };
            match value {
                ReplValue::String(value) => Ok(ReplValue::Int(value.chars().count() as i64)),
                other => Err(format!(
                    "core.string.length expects String, found {}",
                    other.render()
                )),
            }
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringByteSize) => {
            let [ReplValue::String(value)] = args.as_slice() else {
                return Err("core.string.byte_size expects String".to_string());
            };
            Ok(ReplValue::Int(value.len() as i64))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringUppercase) => {
            string_unary(args.as_slice(), "core.string.uppercase", str::to_uppercase)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringLowercase) => {
            string_unary(args.as_slice(), "core.string.lowercase", str::to_lowercase)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringTrim) => {
            string_unary(args.as_slice(), "core.string.trim", |value| {
                value.trim().to_string()
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringTrimStart) => {
            string_unary(args.as_slice(), "core.string.trim_start", |value| {
                value.trim_start().to_string()
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringTrimEnd) => {
            string_unary(args.as_slice(), "core.string.trim_end", |value| {
                value.trim_end().to_string()
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringReplace) => {
            let [ReplValue::String(value), ReplValue::String(from), ReplValue::String(to)] =
                args.as_slice()
            else {
                return Err("core.string.replace expects value, from, and to Strings".to_string());
            };
            Ok(ReplValue::String(value.replace(from, to)))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringSplit) => {
            let [ReplValue::String(value), ReplValue::String(separator)] = args.as_slice() else {
                return Err("core.string.split expects value and separator Strings".to_string());
            };
            Ok(ReplValue::List(
                value
                    .split(separator)
                    .map(|part| ReplValue::String(part.to_string()))
                    .collect(),
            ))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringSplitOnce) => {
            let [ReplValue::String(value), ReplValue::String(separator)] = args.as_slice() else {
                return Err(
                    "core.string.split_once expects value and separator Strings".to_string()
                );
            };
            Ok(value
                .split_once(separator)
                .map(|(left, right)| {
                    some_value(ReplValue::Tuple(vec![
                        ReplValue::String(left.to_string()),
                        ReplValue::String(right.to_string()),
                    ]))
                })
                .unwrap_or_else(none_value))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListNew) => Ok(ReplValue::List(vec![])),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListIsEmpty) => {
            let [ReplValue::List(items)] = args.as_slice() else {
                return Err("core.list.is_empty expects List".to_string());
            };
            Ok(ReplValue::Bool(items.is_empty()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListLength) => {
            let [ReplValue::List(items)] = args.as_slice() else {
                return Err("core.list.length expects List".to_string());
            };
            Ok(ReplValue::Int(items.len() as i64))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListFirst) => {
            let [ReplValue::List(items)] = args.as_slice() else {
                return Err("core.list.first expects List".to_string());
            };
            Ok(items
                .first()
                .cloned()
                .map(some_value)
                .unwrap_or_else(none_value))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListIterator) => {
            let [ReplValue::List(items)] = args.as_slice() else {
                return Err("core.list.iterator expects List".to_string());
            };
            Ok(ReplValue::Iterator {
                items: items.clone(),
                index: 0,
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListPush) => {
            let [ReplValue::List(items), value] = args.as_slice() else {
                return Err("core.list.push expects List and value".to_string());
            };
            let mut updated = items.clone();
            updated.push(value.clone());
            Ok(ReplValue::List(updated))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListClear) => {
            Ok(ReplValue::List(vec![]))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IteratorNext) => {
            iterator_next(args.as_slice())
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapNew) => Ok(ReplValue::Map(vec![])),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapFromEntries) => {
            let [ReplValue::List(entries)] = args.as_slice() else {
                return Err("core.map.from_entries expects List of entries".to_string());
            };
            map_from_entries(entries.clone())
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapIsEmpty) => {
            let [ReplValue::Map(entries)] = args.as_slice() else {
                return Err("core.map.is_empty expects Map".to_string());
            };
            Ok(ReplValue::Bool(entries.is_empty()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapSize) => {
            let [ReplValue::Map(entries)] = args.as_slice() else {
                return Err("core.map.size expects Map".to_string());
            };
            Ok(ReplValue::Int(entries.len() as i64))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapGet) => {
            let [ReplValue::Map(entries), key] = args.as_slice() else {
                return Err("core.map.get expects Map and key".to_string());
            };
            Ok(entries
                .iter()
                .find(|(entry_key, _)| entry_key == key)
                .map(|(_, value)| some_value(value.clone()))
                .unwrap_or_else(none_value))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapContainsKey) => {
            let [ReplValue::Map(entries), key] = args.as_slice() else {
                return Err("core.map.contains_key expects Map and key".to_string());
            };
            Ok(ReplValue::Bool(
                entries.iter().any(|(entry_key, _)| entry_key == key),
            ))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapIterator) => {
            let [ReplValue::Map(entries)] = args.as_slice() else {
                return Err("core.map.iterator expects Map".to_string());
            };
            Ok(ReplValue::Iterator {
                items: entries
                    .iter()
                    .map(|(key, value)| ReplValue::Tuple(vec![key.clone(), value.clone()]))
                    .collect(),
                index: 0,
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapPut) => {
            let [ReplValue::Map(entries), key, value] = args.as_slice() else {
                return Err("core.map.put expects Map, key, and value".to_string());
            };
            let mut updated = entries.clone();
            map_insert(&mut updated, key.clone(), value.clone());
            Ok(ReplValue::Map(updated))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapRemove) => {
            let [ReplValue::Map(entries), key] = args.as_slice() else {
                return Err("core.map.remove expects Map and key".to_string());
            };
            let mut updated = entries.clone();
            updated.retain(|(entry_key, _)| entry_key != key);
            Ok(ReplValue::Map(updated))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapClear) => Ok(ReplValue::Map(vec![])),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetNew) => Ok(ReplValue::Set(vec![])),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetFromList) => {
            let [ReplValue::List(items)] = args.as_slice() else {
                return Err("core.set.from_list expects List".to_string());
            };
            Ok(ReplValue::Set(unique_values(items.clone())))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetIsEmpty) => {
            let [ReplValue::Set(items)] = args.as_slice() else {
                return Err("core.set.is_empty expects Set".to_string());
            };
            Ok(ReplValue::Bool(items.is_empty()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetSize) => {
            let [ReplValue::Set(items)] = args.as_slice() else {
                return Err("core.set.size expects Set".to_string());
            };
            Ok(ReplValue::Int(items.len() as i64))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetContains) => {
            let [ReplValue::Set(items), value] = args.as_slice() else {
                return Err("core.set.contains expects Set and value".to_string());
            };
            Ok(ReplValue::Bool(items.contains(value)))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetIterator) => {
            let [ReplValue::Set(items)] = args.as_slice() else {
                return Err("core.set.iterator expects Set".to_string());
            };
            Ok(ReplValue::Iterator {
                items: items.clone(),
                index: 0,
            })
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetAdd) => {
            let [ReplValue::Set(items), value] = args.as_slice() else {
                return Err("core.set.add expects Set and value".to_string());
            };
            let mut updated = items.clone();
            if !updated.contains(value) {
                updated.push(value.clone());
            }
            Ok(ReplValue::Set(updated))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetRemove) => {
            let [ReplValue::Set(items), value] = args.as_slice() else {
                return Err("core.set.remove expects Set and value".to_string());
            };
            let mut updated = items.clone();
            updated.retain(|item| item != value);
            Ok(ReplValue::Set(updated))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetClear) => Ok(ReplValue::Set(vec![])),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TaskDone) => {
            let [value] = args.as_slice() else {
                return Err("core.task.done expects one value".to_string());
            };
            Ok(ok_value(value.clone()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TaskResult) => {
            let [task] = args.as_slice() else {
                return Err("core.task.result expects one task".to_string());
            };
            Ok(task.clone())
        }
        other => Err(format!(
            "CoreIR evaluator does not yet support intrinsic {:?}",
            other
        )),
    }
}

/// Executes the REPL console print effect hook.
///
/// Inputs:
/// - `value`: already evaluated argument for `std.io.Console.println`.
/// - `output`: callback invoked with the string payload to print.
///
/// Output:
/// - `Unit` after printing a string, or a type-specific evaluator error.
///
/// Transformation:
/// - Implements the required REPL effect hook while keeping target
///   console details out of the public source language and lets the CLI choose
///   text or structured event output.
fn evaluate_console_println(
    value: &ReplValue,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    match value {
        ReplValue::String(value) => {
            output(value);
            Ok(ReplValue::Unit)
        }
        other => Err(format!(
            "std.io.Console.println expects String, found {}",
            other.render()
        )),
    }
}

/// Evaluates a unary operator.
///
/// Inputs:
/// - `operator`: CoreIR operator spelling.
/// - `value`: evaluated operand.
///
/// Output:
/// - Result value or operator/type error text.
///
/// Transformation:
/// - Applies the selected primitive unary operations in target-neutral form.
fn evaluate_unary(operator: &str, value: ReplValue) -> Result<ReplValue, String> {
    match (operator, value) {
        ("-", ReplValue::Int(value)) => Ok(ReplValue::Int(-value)),
        ("not", ReplValue::Bool(value)) => Ok(ReplValue::Bool(!value)),
        (operator, value) => Err(format!(
            "unsupported unary operator `{operator}` for {}",
            value.render()
        )),
    }
}

/// Evaluates a binary operator.
///
/// Inputs:
/// - `operator`: CoreIR operator spelling.
/// - `left`: evaluated left operand.
/// - `right`: evaluated right operand.
///
/// Output:
/// - Result value or operator/type error text.
///
/// Transformation:
/// - Applies selected arithmetic, comparison, equality, string append, and
///   boolean operators in target-neutral form.
fn evaluate_binary(operator: &str, left: ReplValue, right: ReplValue) -> Result<ReplValue, String> {
    match (operator, left, right) {
        ("+", ReplValue::Int(left), ReplValue::Int(right)) => Ok(ReplValue::Int(left + right)),
        ("-", ReplValue::Int(left), ReplValue::Int(right)) => Ok(ReplValue::Int(left - right)),
        ("*", ReplValue::Int(left), ReplValue::Int(right)) => Ok(ReplValue::Int(left * right)),
        ("div", ReplValue::Int(left), ReplValue::Int(right))
        | ("/", ReplValue::Int(left), ReplValue::Int(right)) => Ok(ReplValue::Int(left / right)),
        ("+", ReplValue::String(left), ReplValue::String(right)) => {
            Ok(ReplValue::String(format!("{left}{right}")))
        }
        ("==", left, right) => Ok(ReplValue::Bool(left == right)),
        ("!=", left, right) => Ok(ReplValue::Bool(left != right)),
        (">", ReplValue::Int(left), ReplValue::Int(right)) => Ok(ReplValue::Bool(left > right)),
        (">=", ReplValue::Int(left), ReplValue::Int(right)) => Ok(ReplValue::Bool(left >= right)),
        ("<", ReplValue::Int(left), ReplValue::Int(right)) => Ok(ReplValue::Bool(left < right)),
        ("<=", ReplValue::Int(left), ReplValue::Int(right)) => Ok(ReplValue::Bool(left <= right)),
        ("and" | "&&", ReplValue::Bool(left), ReplValue::Bool(right)) => {
            Ok(ReplValue::Bool(left && right))
        }
        ("or" | "||", ReplValue::Bool(left), ReplValue::Bool(right)) => {
            Ok(ReplValue::Bool(left || right))
        }
        (operator, left, right) => Err(format!(
            "unsupported binary operator `{operator}` for {} and {}",
            left.render(),
            right.render()
        )),
    }
}

/// Applies a string-to-string intrinsic helper.
///
/// Inputs:
/// - `args`: evaluated intrinsic arguments.
/// - `name`: stable intrinsic name for diagnostics.
/// - `operation`: pure string transformation.
///
/// Output:
/// - String result or arity/type error.
///
/// Transformation:
/// - Reuses one checked path for unary string operations.
fn string_unary(
    args: &[ReplValue],
    name: &str,
    operation: fn(&str) -> String,
) -> Result<ReplValue, String> {
    let [value] = args else {
        return Err(format!("{name} expects one argument"));
    };
    match value {
        ReplValue::String(value) => Ok(ReplValue::String(operation(value))),
        other => Err(format!("{name} expects String, found {}", other.render())),
    }
}

/// Applies a string predicate that takes one string argument.
fn string_predicate(
    args: &[ReplValue],
    name: &str,
    operation: fn(&str) -> bool,
) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err(format!("{name} expects String"));
    };
    Ok(ReplValue::Bool(operation(value)))
}

/// Applies a string predicate that takes two string arguments.
fn string_binary_predicate(
    args: &[ReplValue],
    name: &str,
    operation: fn(&str, &str) -> bool,
) -> Result<ReplValue, String> {
    let [ReplValue::String(value), ReplValue::String(pattern)] = args else {
        return Err(format!("{name} expects two Strings"));
    };
    Ok(ReplValue::Bool(operation(value, pattern)))
}

/// Advances a VM iterator value.
fn iterator_next(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Iterator { items, index }] = args else {
        return Err("core.iterator.next expects Iterator".to_string());
    };
    let Some(value) = items.get(*index).cloned() else {
        return Ok(none_value());
    };
    Ok(some_value(ReplValue::Tuple(vec![
        value,
        ReplValue::Iterator {
            items: items.clone(),
            index: index + 1,
        },
    ])))
}

/// Builds `Some(value)`.
fn some_value(value: ReplValue) -> ReplValue {
    ReplValue::Tuple(vec![ReplValue::Atom("some".to_string()), value])
}

/// Builds `None`.
fn none_value() -> ReplValue {
    ReplValue::Atom("none".to_string())
}

/// Builds `Ok(value)`.
fn ok_value(value: ReplValue) -> ReplValue {
    ReplValue::Tuple(vec![ReplValue::Atom("ok".to_string()), value])
}

/// Builds `Err(reason)`.
fn err_value(reason: ReplValue) -> ReplValue {
    ReplValue::Tuple(vec![ReplValue::Atom("error".to_string()), reason])
}

/// Builds a portable file error result value.
fn file_error_value(path: &str, err: &std::io::Error) -> ReplValue {
    let code = match err.kind() {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidData => "invalid_path",
        _ => "unknown",
    };
    err_value(file_error_record(code, &err.to_string(), path))
}

/// Builds the VM's compact `std.io.File.FileError` representation.
fn file_error_record(code: &str, message: &str, path: &str) -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Atom("file_error".to_string()),
        ReplValue::Atom(code.to_string()),
        ReplValue::String(message.to_string()),
        ReplValue::String(path.to_string()),
    ])
}

/// Returns unique values while preserving first-seen order.
fn unique_values(values: Vec<ReplValue>) -> Vec<ReplValue> {
    let mut unique = Vec::new();
    for value in values {
        if !unique.contains(&value) {
            unique.push(value);
        }
    }
    unique
}

/// Compares two Bool values into `std.core.Ordering.Comparison`.
fn compare_bool(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Bool(left), ReplValue::Bool(right)] = args else {
        return Err("core.bool.compare expects two Bool values".to_string());
    };
    Ok(ordering_atom(left.cmp(right)))
}

/// Compares two String values into `std.core.Ordering.Comparison`.
fn compare_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(left), ReplValue::String(right)] = args else {
        return Err("core.string.compare expects two String values".to_string());
    };
    Ok(ordering_atom(left.cmp(right)))
}

/// Converts Rust ordering into the Terlan comparison atom value.
fn ordering_atom(ordering: std::cmp::Ordering) -> ReplValue {
    let atom = match ordering {
        std::cmp::Ordering::Less => "lt",
        std::cmp::Ordering::Equal => "eq",
        std::cmp::Ordering::Greater => "gt",
    };
    ReplValue::Atom(atom.to_string())
}

/// Evaluates stdlib remote calls that are not loaded as executable modules.
fn evaluate_std_remote(
    module: &str,
    function: &str,
    args: Vec<ReplValue>,
) -> Result<ReplValue, String> {
    if module == "__receiver__" {
        return evaluate_receiver_remote(function, args);
    }
    match (module, function) {
        ("List" | "std.collections.List", "new") => Ok(ReplValue::List(vec![])),
        ("Map" | "std.collections.Map" | "Object" | "std.core.Object", "new") => {
            Ok(ReplValue::Map(vec![]))
        }
        ("Set" | "std.collections.Set", "new") => Ok(ReplValue::Set(vec![])),
        ("Map" | "std.collections.Map" | "Object" | "std.core.Object", "from_entries") => {
            let [ReplValue::List(entries)] = args.as_slice() else {
                return Err(format!("{module}.from_entries expects List"));
            };
            map_from_entries(entries.clone())
        }
        ("Set" | "std.collections.Set", "from_list") => {
            let [ReplValue::List(items)] = args.as_slice() else {
                return Err(format!("{module}.from_list expects List"));
            };
            Ok(ReplValue::Set(unique_values(items.clone())))
        }
        ("std.test.Test", "assert") | ("std.test.Test", "assert_true") => {
            expect_unary_bool(module, function, args)
        }
        ("std.test.Test", "assert_false") => {
            let value = expect_unary_bool(module, function, args)?;
            match value {
                ReplValue::Bool(value) => Ok(ReplValue::Bool(!value)),
                _ => unreachable!("expect_unary_bool returns Bool"),
            }
        }
        ("std.test.Test", "assert_equal") => expect_binary_equal(args, true),
        ("std.test.Test", "assert_not_equal") => expect_binary_equal(args, false),
        ("std.test.Test", "fail") if args.is_empty() => Ok(ReplValue::Bool(false)),
        ("std.core.Bool", "equal") => expect_binary_equal(args, true),
        ("std.core.Bool", "is_true") => expect_unary_bool(module, function, args),
        ("std.core.Bool", "is_false") => {
            let value = expect_unary_bool(module, function, args)?;
            let ReplValue::Bool(value) = value else {
                unreachable!("expect_unary_bool returns Bool");
            };
            Ok(ReplValue::Bool(!value))
        }
        ("std.core.Bool", "compare") => compare_bool(args.as_slice()),
        ("std.core.Bool", "to_string") => bool_to_string(args.as_slice()),
        ("std.core.Bool", "from_string") => bool_from_string(args.as_slice()),
        ("std.core.Int", "equal") => expect_binary_equal(args, true),
        ("std.core.Int", "min") => int_minmax(args.as_slice(), true),
        ("std.core.Int", "max") => int_minmax(args.as_slice(), false),
        ("std.core.Int", "abs") => int_abs(args.as_slice()),
        ("std.core.Int", "compare") => int_compare(args.as_slice()),
        ("std.core.Int", "to_string") => int_to_string(args.as_slice()),
        ("std.core.Int", "from_string") => int_from_string(args.as_slice()),
        ("std.core.Float", "equal") => expect_binary_equal(args, true),
        ("std.core.Float", "min") => float_minmax(args.as_slice(), true),
        ("std.core.Float", "max") => float_minmax(args.as_slice(), false),
        ("std.core.Float", "abs") => float_abs(args.as_slice()),
        ("std.core.Float", "compare") => float_compare(args.as_slice()),
        ("std.core.Float", "to_string") => float_to_string(args.as_slice()),
        ("std.core.Float", "from_string") => float_from_string(args.as_slice()),
        ("std.core.String", "equal") => expect_binary_equal(args, true),
        ("std.core.String", "compare") => compare_string(args.as_slice()),
        ("std.core.String", "to_string") => string_identity(args.as_slice()),
        ("std.core.String", "from_string") => string_from_string(args.as_slice()),
        ("std.core.String", "is_empty") => {
            string_predicate(args.as_slice(), "std.core.String.is_empty", |value| {
                value.is_empty()
            })
        }
        ("std.core.String", "append") => string_append(args.as_slice()),
        ("std.core.String", "concat") => string_concat(args.as_slice()),
        ("std.core.String", "contains") => string_binary_predicate(
            args.as_slice(),
            "std.core.String.contains",
            |value, pattern| value.contains(pattern),
        ),
        ("std.core.String", "starts_with") => string_binary_predicate(
            args.as_slice(),
            "std.core.String.starts_with",
            |value, pattern| value.starts_with(pattern),
        ),
        ("std.core.String", "ends_with") => string_binary_predicate(
            args.as_slice(),
            "std.core.String.ends_with",
            |value, pattern| value.ends_with(pattern),
        ),
        ("std.core.String", "length") => string_length(args.as_slice(), false),
        ("std.core.String", "byte_size") => string_length(args.as_slice(), true),
        ("std.core.String", "lowercase") => string_unary(
            args.as_slice(),
            "std.core.String.lowercase",
            str::to_lowercase,
        ),
        ("std.core.String", "uppercase") => string_unary(
            args.as_slice(),
            "std.core.String.uppercase",
            str::to_uppercase,
        ),
        ("std.core.String", "trim") => {
            string_unary(args.as_slice(), "std.core.String.trim", |value| {
                value.trim().to_string()
            })
        }
        ("std.core.String", "trim_start") => {
            string_unary(args.as_slice(), "std.core.String.trim_start", |value| {
                value.trim_start().to_string()
            })
        }
        ("std.core.String", "trim_end") => {
            string_unary(args.as_slice(), "std.core.String.trim_end", |value| {
                value.trim_end().to_string()
            })
        }
        ("std.core.String", "replace") => string_replace(args.as_slice()),
        ("std.core.String", "split") => string_split(args.as_slice()),
        ("std.core.String", "split_once") => string_split_once(args.as_slice()),
        ("std.core.Option", "is_some") => option_is_some(args.as_slice()),
        ("std.core.Option", "is_none") => option_is_none(args.as_slice()),
        ("std.core.Option", "with_default") => option_with_default(args.as_slice()),
        ("std.core.Result", "is_ok") => result_is_ok(args.as_slice()),
        ("std.core.Result", "is_err") => result_is_err(args.as_slice()),
        ("std.core.Result", "with_default") => result_with_default(args.as_slice()),
        ("std.core.Unit", "equal") => Ok(ReplValue::Bool(true)),
        ("std.core.Unit", "compare") => Ok(ReplValue::Atom("eq".to_string())),
        ("std.core.Unit", "to_string") => Ok(ReplValue::String("unit".to_string())),
        ("std.core.Unit", "from_string") => unit_from_string(args.as_slice()),
        ("std.core.Ordering", "compare") => ordering_compare(args.as_slice()),
        ("std.core.Ordering", "to_string") => atom_payload_to_string(args.as_slice()),
        ("std.core.Ordering", "from_string") => ordering_from_string(args.as_slice()),
        ("std.core.Atom", "equal") => expect_binary_equal(args, true),
        ("std.core.Atom", "to_string") => atom_payload_to_string(args.as_slice()),
        ("std.io.File", "new") => file_new(args.as_slice()),
        ("std.io.File", "code") => file_error_field(args.as_slice(), 1, "code"),
        ("std.io.File", "message") => file_error_field(args.as_slice(), 2, "message"),
        ("std.io.File", "path") => file_error_field(args.as_slice(), 3, "path"),
        _ => Err(format!(
            "CoreIR evaluator does not yet support RemoteCall {module}:{function}/{}",
            args.len()
        )),
    }
}

/// Evaluates a VM-facing dynamic receiver call.
fn evaluate_receiver_remote(function: &str, args: Vec<ReplValue>) -> Result<ReplValue, String> {
    let Some((receiver, rest)) = args.split_first() else {
        return Err(format!("receiver call `{function}` requires a receiver"));
    };
    match (function, receiver, rest) {
        ("is_empty", ReplValue::String(value), []) => Ok(ReplValue::Bool(value.is_empty())),
        ("is_empty", ReplValue::List(items), []) => Ok(ReplValue::Bool(items.is_empty())),
        ("is_empty", ReplValue::Map(entries), []) => Ok(ReplValue::Bool(entries.is_empty())),
        ("is_empty", ReplValue::Set(items), []) => Ok(ReplValue::Bool(items.is_empty())),
        ("length", ReplValue::String(value), []) => {
            Ok(ReplValue::Int(value.chars().count() as i64))
        }
        ("length", ReplValue::List(items), []) => Ok(ReplValue::Int(items.len() as i64)),
        ("byte_size", ReplValue::String(value), []) => Ok(ReplValue::Int(value.len() as i64)),
        ("size", ReplValue::Map(entries), []) => Ok(ReplValue::Int(entries.len() as i64)),
        ("size", ReplValue::Set(items), []) => Ok(ReplValue::Int(items.len() as i64)),
        ("first", ReplValue::List(items), []) => Ok(items
            .first()
            .cloned()
            .map(some_value)
            .unwrap_or_else(none_value)),
        ("iterator", ReplValue::List(items), []) | ("iterator", ReplValue::Set(items), []) => {
            Ok(ReplValue::Iterator {
                items: items.clone(),
                index: 0,
            })
        }
        ("iterator", ReplValue::Map(entries), []) => Ok(ReplValue::Iterator {
            items: entries
                .iter()
                .map(|(key, value)| ReplValue::Tuple(vec![key.clone(), value.clone()]))
                .collect(),
            index: 0,
        }),
        ("get", ReplValue::Map(entries), [key]) => Ok(entries
            .iter()
            .find(|(entry_key, _)| entry_key == key)
            .map(|(_, value)| some_value(value.clone()))
            .unwrap_or_else(none_value)),
        ("contains_key", ReplValue::Map(entries), [key]) => Ok(ReplValue::Bool(
            entries.iter().any(|(entry_key, _)| entry_key == key),
        )),
        ("contains", ReplValue::Set(items), [value]) => Ok(ReplValue::Bool(items.contains(value))),
        ("contains", ReplValue::String(value), [ReplValue::String(pattern)]) => {
            Ok(ReplValue::Bool(value.contains(pattern)))
        }
        ("starts_with", ReplValue::String(value), [ReplValue::String(pattern)]) => {
            Ok(ReplValue::Bool(value.starts_with(pattern)))
        }
        ("ends_with", ReplValue::String(value), [ReplValue::String(pattern)]) => {
            Ok(ReplValue::Bool(value.ends_with(pattern)))
        }
        ("append", ReplValue::String(value), [ReplValue::String(suffix)]) => {
            Ok(ReplValue::String(format!("{value}{suffix}")))
        }
        ("lowercase", ReplValue::String(value), []) => Ok(ReplValue::String(value.to_lowercase())),
        ("uppercase", ReplValue::String(value), []) => Ok(ReplValue::String(value.to_uppercase())),
        ("trim", ReplValue::String(value), []) => Ok(ReplValue::String(value.trim().to_string())),
        ("trim_start", ReplValue::String(value), []) => {
            Ok(ReplValue::String(value.trim_start().to_string()))
        }
        ("trim_end", ReplValue::String(value), []) => {
            Ok(ReplValue::String(value.trim_end().to_string()))
        }
        ("replace", ReplValue::String(value), [ReplValue::String(from), ReplValue::String(to)]) => {
            Ok(ReplValue::String(value.replace(from, to)))
        }
        ("split", ReplValue::String(value), [ReplValue::String(separator)]) => Ok(ReplValue::List(
            value
                .split(separator)
                .map(|part| ReplValue::String(part.to_string()))
                .collect(),
        )),
        ("split_once", ReplValue::String(value), [ReplValue::String(separator)]) => Ok(value
            .split_once(separator)
            .map(|(left, right)| {
                some_value(ReplValue::Tuple(vec![
                    ReplValue::String(left.to_string()),
                    ReplValue::String(right.to_string()),
                ]))
            })
            .unwrap_or_else(none_value)),
        ("to_string", value, []) => Ok(ReplValue::String(match value {
            ReplValue::String(value) => value.clone(),
            ReplValue::Bool(value) => value.to_string(),
            ReplValue::Int(value) => value.to_string(),
            ReplValue::Float(value) => value.clone(),
            ReplValue::Atom(value) => value.clone(),
            ReplValue::Unit => "unit".to_string(),
            other => other.render(),
        })),
        _ => Err(format!(
            "CoreIR evaluator does not yet support receiver call `{function}` for {}",
            receiver.render()
        )),
    }
}

/// Expects and returns one Bool argument.
fn expect_unary_bool(
    module: &str,
    function: &str,
    args: Vec<ReplValue>,
) -> Result<ReplValue, String> {
    let [ReplValue::Bool(value)] = args.as_slice() else {
        return Err(format!("{module}.{function} expects Bool"));
    };
    Ok(ReplValue::Bool(*value))
}

/// Evaluates binary equality or inequality.
fn expect_binary_equal(args: Vec<ReplValue>, expected_equal: bool) -> Result<ReplValue, String> {
    let [left, right] = args.as_slice() else {
        return Err("equality helper expects two arguments".to_string());
    };
    Ok(ReplValue::Bool((left == right) == expected_equal))
}

/// Converts Bool to String.
fn bool_to_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Bool(value)] = args else {
        return Err("Bool.to_string expects Bool".to_string());
    };
    Ok(ReplValue::String(value.to_string()))
}

/// Parses Bool from String.
fn bool_from_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("Bool.from_string expects String".to_string());
    };
    Ok(match value.as_str() {
        "true" => some_value(ReplValue::Bool(true)),
        "false" => some_value(ReplValue::Bool(false)),
        _ => none_value(),
    })
}

/// Returns integer min or max.
fn int_minmax(args: &[ReplValue], min: bool) -> Result<ReplValue, String> {
    let [ReplValue::Int(left), ReplValue::Int(right)] = args else {
        return Err("Int min/max expects two Int values".to_string());
    };
    Ok(ReplValue::Int(if min {
        (*left).min(*right)
    } else {
        (*left).max(*right)
    }))
}

/// Returns integer absolute value.
fn int_abs(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Int(value)] = args else {
        return Err("Int.abs expects Int".to_string());
    };
    Ok(ReplValue::Int(value.abs()))
}

/// Compares Int values.
fn int_compare(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Int(left), ReplValue::Int(right)] = args else {
        return Err("Int.compare expects two Int values".to_string());
    };
    Ok(ordering_atom(left.cmp(right)))
}

/// Converts Int to String.
fn int_to_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Int(value)] = args else {
        return Err("Int.to_string expects Int".to_string());
    };
    Ok(ReplValue::String(value.to_string()))
}

/// Parses Int from String.
fn int_from_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("Int.from_string expects String".to_string());
    };
    Ok(value
        .parse::<i64>()
        .map(|value| some_value(ReplValue::Int(value)))
        .unwrap_or_else(|_| none_value()))
}

/// Parses a VM float payload.
fn parse_float(value: &str) -> Result<f64, String> {
    value
        .parse::<f64>()
        .map_err(|err| format!("invalid Float `{value}`: {err}"))
}

/// Returns float min or max.
fn float_minmax(args: &[ReplValue], min: bool) -> Result<ReplValue, String> {
    let [ReplValue::Float(left), ReplValue::Float(right)] = args else {
        return Err("Float min/max expects two Float values".to_string());
    };
    let left_value = parse_float(left)?;
    let right_value = parse_float(right)?;
    Ok(ReplValue::Float(if (left_value <= right_value) == min {
        left.clone()
    } else {
        right.clone()
    }))
}

/// Returns float absolute value.
fn float_abs(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Float(value)] = args else {
        return Err("Float.abs expects Float".to_string());
    };
    let parsed = parse_float(value)?.abs();
    Ok(ReplValue::Float(parsed.to_string()))
}

/// Compares Float values.
fn float_compare(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Float(left), ReplValue::Float(right)] = args else {
        return Err("Float.compare expects two Float values".to_string());
    };
    let ordering = parse_float(left)?
        .partial_cmp(&parse_float(right)?)
        .ok_or_else(|| "Float.compare does not support non-finite values".to_string())?;
    Ok(ordering_atom(ordering))
}

/// Converts Float to String.
fn float_to_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Float(value)] = args else {
        return Err("Float.to_string expects Float".to_string());
    };
    Ok(ReplValue::String(value.clone()))
}

/// Parses Float from String.
fn float_from_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("Float.from_string expects String".to_string());
    };
    Ok(value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map(|_| some_value(ReplValue::Float(value.clone())))
        .unwrap_or_else(none_value))
}

/// Returns a String unchanged.
fn string_identity(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("String.to_string expects String".to_string());
    };
    Ok(ReplValue::String(value.clone()))
}

/// Wraps a String in Some.
fn string_from_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    string_identity(args).map(some_value)
}

/// Appends two Strings.
fn string_append(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(left), ReplValue::String(right)] = args else {
        return Err("String.append expects two Strings".to_string());
    };
    Ok(ReplValue::String(format!("{left}{right}")))
}

/// Concatenates a list of strings.
fn string_concat(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::List(values)] = args else {
        return Err("String.concat expects List[String]".to_string());
    };
    let mut result = String::new();
    for value in values {
        let ReplValue::String(value) = value else {
            return Err(format!(
                "String.concat item must be String, found {}",
                value.render()
            ));
        };
        result.push_str(value);
    }
    Ok(ReplValue::String(result))
}

/// Returns String char or byte length.
fn string_length(args: &[ReplValue], bytes: bool) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("String length expects String".to_string());
    };
    Ok(ReplValue::Int(if bytes {
        value.len()
    } else {
        value.chars().count()
    } as i64))
}

/// Replaces all String matches.
fn string_replace(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value), ReplValue::String(from), ReplValue::String(to)] = args else {
        return Err("String.replace expects value, from, and to".to_string());
    };
    Ok(ReplValue::String(value.replace(from, to)))
}

/// Splits a String.
fn string_split(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value), ReplValue::String(separator)] = args else {
        return Err("String.split expects value and separator".to_string());
    };
    Ok(ReplValue::List(
        value
            .split(separator)
            .map(|part| ReplValue::String(part.to_string()))
            .collect(),
    ))
}

/// Splits a String once.
fn string_split_once(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value), ReplValue::String(separator)] = args else {
        return Err("String.split_once expects value and separator".to_string());
    };
    Ok(value
        .split_once(separator)
        .map(|(left, right)| {
            some_value(ReplValue::Tuple(vec![
                ReplValue::String(left.to_string()),
                ReplValue::String(right.to_string()),
            ]))
        })
        .unwrap_or_else(none_value))
}

/// Tests Option presence.
fn option_is_some(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [value] = args else {
        return Err("Option.is_some expects one argument".to_string());
    };
    Ok(ReplValue::Bool(matches!(
        value,
        ReplValue::Tuple(items)
            if matches!(items.as_slice(), [ReplValue::Atom(tag), _] if tag == "some")
    )))
}

/// Tests Option absence.
fn option_is_none(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [value] = args else {
        return Err("Option.is_none expects one argument".to_string());
    };
    Ok(ReplValue::Bool(
        matches!(value, ReplValue::Atom(tag) if tag == "none"),
    ))
}

/// Unwraps Option with a default.
fn option_with_default(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [value, default] = args else {
        return Err("Option.with_default expects value and default".to_string());
    };
    match value {
        ReplValue::Tuple(items) => match items.as_slice() {
            [ReplValue::Atom(tag), value] if tag == "some" => Ok(value.clone()),
            _ => Ok(default.clone()),
        },
        ReplValue::Atom(tag) if tag == "none" => Ok(default.clone()),
        _ => Ok(default.clone()),
    }
}

/// Tests Result success.
fn result_is_ok(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [value] = args else {
        return Err("Result.is_ok expects one argument".to_string());
    };
    Ok(ReplValue::Bool(matches!(
        value,
        ReplValue::Tuple(items)
            if matches!(items.as_slice(), [ReplValue::Atom(tag), _] if tag == "ok")
    )))
}

/// Tests Result error.
fn result_is_err(args: &[ReplValue]) -> Result<ReplValue, String> {
    result_is_ok(args).map(|value| match value {
        ReplValue::Bool(value) => ReplValue::Bool(!value),
        other => other,
    })
}

/// Unwraps Result with a default.
fn result_with_default(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [value, default] = args else {
        return Err("Result.with_default expects value and default".to_string());
    };
    match value {
        ReplValue::Tuple(items) => match items.as_slice() {
            [ReplValue::Atom(tag), value] if tag == "ok" => Ok(value.clone()),
            _ => Ok(default.clone()),
        },
        _ => Ok(default.clone()),
    }
}

/// Parses Unit from String.
fn unit_from_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("Unit.from_string expects String".to_string());
    };
    Ok(if value == "unit" {
        some_value(ReplValue::Unit)
    } else {
        none_value()
    })
}

/// Compares Ordering atoms.
fn ordering_compare(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Atom(left), ReplValue::Atom(right)] = args else {
        return Err("Ordering.compare expects two comparison atoms".to_string());
    };
    let rank = |value: &str| match value {
        "lt" => Some(0),
        "eq" => Some(1),
        "gt" => Some(2),
        _ => None,
    };
    let left = rank(left).ok_or_else(|| "unknown left Ordering atom".to_string())?;
    let right = rank(right).ok_or_else(|| "unknown right Ordering atom".to_string())?;
    Ok(ordering_atom(left.cmp(&right)))
}

/// Converts an atom payload to String.
fn atom_payload_to_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Atom(value)] = args else {
        return Err("atom to_string expects Atom".to_string());
    };
    Ok(ReplValue::String(value.clone()))
}

/// Parses Ordering from String.
fn ordering_from_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(value)] = args else {
        return Err("Ordering.from_string expects String".to_string());
    };
    Ok(match value.as_str() {
        "lt" | "eq" | "gt" => some_value(ReplValue::Atom(value.clone())),
        _ => none_value(),
    })
}

/// Builds a compact FileError value.
fn file_new(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Atom(code), ReplValue::String(message), ReplValue::String(path)] = args else {
        return Err("File.new expects Atom, String, String".to_string());
    };
    Ok(file_error_record(code, message, path))
}

/// Reads a compact FileError field.
fn file_error_field(args: &[ReplValue], index: usize, name: &str) -> Result<ReplValue, String> {
    let [ReplValue::Tuple(fields)] = args else {
        return Err(format!("File.{name} expects FileError"));
    };
    fields
        .get(index)
        .cloned()
        .ok_or_else(|| format!("File.{name} received malformed FileError"))
}

/// Finds a function by name and arity in a CoreIR module.
///
/// Inputs:
/// - `core`: containing CoreIR module.
/// - `name`: function name to resolve.
/// - `arity`: function arity to resolve.
///
/// Output:
/// - Matching function reference, if present.
///
/// Transformation:
/// - Performs deterministic linear lookup over the module function table.
fn find_function<'a>(core: &'a CoreModule, name: &str, arity: usize) -> Option<&'a CoreFunction> {
    core.functions
        .iter()
        .find(|function| function.name == name && function.arity == arity)
}

/// Returns a compact name for an unsupported CoreIR expression.
///
/// Inputs:
/// - `expr`: unsupported CoreIR expression.
///
/// Output:
/// - Stable variant-like label for diagnostics.
///
/// Transformation:
/// - Maps broad CoreIR shapes to readable evaluator diagnostics.
fn core_expr_kind(expr: &CoreExpr) -> &'static str {
    match expr {
        CoreExpr::Int(_) => "Int",
        CoreExpr::Float(_) => "Float",
        CoreExpr::Binary(_) => "Binary",
        CoreExpr::Atom(_) => "Atom",
        CoreExpr::Var(_) => "Var",
        CoreExpr::Tuple(_) => "Tuple",
        CoreExpr::List(_) => "List",
        CoreExpr::ListCons { .. } => "ListCons",
        CoreExpr::FixedArray(_) => "FixedArray",
        CoreExpr::Index { .. } => "Index",
        CoreExpr::ListComprehension { .. } => "ListComprehension",
        CoreExpr::Let { .. } => "Let",
        CoreExpr::Map(_) => "Map",
        CoreExpr::RecordConstruct { .. } => "RecordConstruct",
        CoreExpr::FieldAccess { .. } => "FieldAccess",
        CoreExpr::RecordAccess { .. } => "RecordAccess",
        CoreExpr::RecordUpdate { .. } => "RecordUpdate",
        CoreExpr::TemplateInstantiate { .. } => "TemplateInstantiate",
        CoreExpr::ConstructorChain { .. } => "ConstructorChain",
        CoreExpr::RemoteFunRef { .. } => "RemoteFunRef",
        CoreExpr::RemoteCall { .. } => "RemoteCall",
        CoreExpr::ConstructorCall { .. } => "ConstructorCall",
        CoreExpr::Call { .. } => "Call",
        CoreExpr::MutableReceiverCall { .. } => "MutableReceiverCall",
        CoreExpr::FunctionCall { .. } => "FunctionCall",
        CoreExpr::Cast { .. } => "Cast",
        CoreExpr::Intrinsic(_) => "Intrinsic",
        CoreExpr::SqlQuery { .. } => "SqlQuery",
        CoreExpr::Case { .. } => "Case",
        CoreExpr::Try { .. } => "Try",
        CoreExpr::If { .. } => "If",
        CoreExpr::Lam { .. } => "Lam",
        CoreExpr::UnaryOp { .. } => "UnaryOp",
        CoreExpr::BinaryOp { .. } => "BinaryOp",
    }
}

/// Returns a compact name for an unsupported CoreIR pattern.
///
/// Inputs:
/// - `pattern`: unsupported function-call pattern.
///
/// Output:
/// - Stable variant-like label for diagnostics.
///
/// Transformation:
/// - Maps broad pattern shapes to readable evaluator diagnostics.
fn core_pattern_kind(pattern: &CorePattern) -> &'static str {
    match pattern {
        CorePattern::Wildcard => "Wildcard",
        CorePattern::Var(_) => "Var",
        CorePattern::Int(_) => "Int",
        CorePattern::Float(_) => "Float",
        CorePattern::Atom(_) => "Atom",
        CorePattern::Tuple(_) => "Tuple",
        CorePattern::List(_) => "List",
        CorePattern::ListCons { .. } => "ListCons",
        CorePattern::Map(_) => "Map",
        CorePattern::Record { .. } => "Record",
        CorePattern::Constructor { .. } => "Constructor",
    }
}

#[cfg(test)]
#[path = "vm_test.rs"]
mod vm_test;
