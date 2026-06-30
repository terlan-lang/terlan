use std::collections::HashMap;

use crate::terlan_typeck::{CoreCaseClause, CoreExpr, CoreFunction, CoreModule, CorePattern};

mod intrinsics;
mod kind;
mod patterns;
mod std_remote;
mod value;

use intrinsics::evaluate_intrinsic;
use kind::{core_expr_kind, core_pattern_kind};
use patterns::{bind_case_pattern, bind_repl_pattern};
use std_remote::evaluate_std_remote;
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
pub(super) fn to_atom_payload(name: &str) -> String {
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
pub(super) fn evaluate_exprs(
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
pub(super) fn map_from_entries(entries: Vec<ReplValue>) -> Result<ReplValue, String> {
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
pub(super) fn map_insert(
    entries: &mut Vec<(ReplValue, ReplValue)>,
    key: ReplValue,
    value: ReplValue,
) {
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
pub(super) fn evaluate_console_println(
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
pub(super) fn string_unary(
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
pub(super) fn string_predicate(
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
pub(super) fn string_binary_predicate(
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
pub(super) fn iterator_next(args: &[ReplValue]) -> Result<ReplValue, String> {
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
pub(super) fn some_value(value: ReplValue) -> ReplValue {
    ReplValue::Tuple(vec![ReplValue::Atom("some".to_string()), value])
}

/// Builds `None`.
pub(super) fn none_value() -> ReplValue {
    ReplValue::Atom("none".to_string())
}

/// Builds `Ok(value)`.
pub(super) fn ok_value(value: ReplValue) -> ReplValue {
    ReplValue::Tuple(vec![ReplValue::Atom("ok".to_string()), value])
}

/// Builds `Err(reason)`.
fn err_value(reason: ReplValue) -> ReplValue {
    ReplValue::Tuple(vec![ReplValue::Atom("error".to_string()), reason])
}

/// Builds a portable file error result value.
pub(super) fn file_error_value(path: &str, err: &std::io::Error) -> ReplValue {
    let code = match err.kind() {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidData => "invalid_path",
        _ => "unknown",
    };
    err_value(file_error_record(code, &err.to_string(), path))
}

/// Builds the VM's compact `std.io.File.FileError` representation.
pub(super) fn file_error_record(code: &str, message: &str, path: &str) -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Atom("file_error".to_string()),
        ReplValue::Atom(code.to_string()),
        ReplValue::String(message.to_string()),
        ReplValue::String(path.to_string()),
    ])
}

/// Returns unique values while preserving first-seen order.
pub(super) fn unique_values(values: Vec<ReplValue>) -> Vec<ReplValue> {
    let mut unique = Vec::new();
    for value in values {
        if !unique.contains(&value) {
            unique.push(value);
        }
    }
    unique
}

/// Compares two Bool values into `std.core.Ordering.Comparison`.
pub(super) fn compare_bool(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::Bool(left), ReplValue::Bool(right)] = args else {
        return Err("core.bool.compare expects two Bool values".to_string());
    };
    Ok(ordering_atom(left.cmp(right)))
}

/// Compares two String values into `std.core.Ordering.Comparison`.
pub(super) fn compare_string(args: &[ReplValue]) -> Result<ReplValue, String> {
    let [ReplValue::String(left), ReplValue::String(right)] = args else {
        return Err("core.string.compare expects two String values".to_string());
    };
    Ok(ordering_atom(left.cmp(right)))
}

/// Converts Rust ordering into the Terlan comparison atom value.
pub(super) fn ordering_atom(ordering: std::cmp::Ordering) -> ReplValue {
    let atom = match ordering {
        std::cmp::Ordering::Less => "lt",
        std::cmp::Ordering::Equal => "eq",
        std::cmp::Ordering::Greater => "gt",
    };
    ReplValue::Atom(atom.to_string())
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
