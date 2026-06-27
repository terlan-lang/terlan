use std::collections::HashMap;

use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreIntrinsicId, CoreModule, CorePattern, CorePrimitiveIntrinsic,
    CoreRuntimeCapability,
};

/// Runtime value produced by the compiler-owned REPL evaluator.
///
/// Inputs:
/// - Constructed from supported CoreIR expressions.
///
/// Output:
/// - A backend-neutral value that can be rendered for the public REPL.
///
/// Transformation:
/// - Keeps REPL execution independent from BEAM/Erlang runtime values while the
///   evaluator grows toward full CoreIR coverage.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ReplValue {
    Unit,
    Int(i64),
    Float(String),
    String(String),
    Atom(String),
    Bool(bool),
    Type(String),
    Tuple(Vec<ReplValue>),
    List(Vec<ReplValue>),
    Closure(ReplClosure),
}

/// Captured anonymous function value for the compiler-owned REPL evaluator.
///
/// Inputs:
/// - `params`: CoreIR lambda parameter patterns.
/// - `body`: CoreIR body evaluated when the closure is applied.
/// - `env`: lexical environment captured when the lambda expression evaluated.
///
/// Output:
/// - Stored inside `ReplValue::Closure` until a function-value call applies it.
///
/// Transformation:
/// - Preserves enough lexical state for REPL lambdas without lowering through a
///   target runtime or exposing backend function values.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReplClosure {
    params: Vec<CorePattern>,
    body: CoreExpr,
    env: HashMap<String, ReplValue>,
}

impl ReplValue {
    /// Renders a REPL value with Terlan source-facing spelling.
    ///
    /// Inputs:
    /// - `self`: evaluated backend-neutral value.
    ///
    /// Output:
    /// - Stable text shown in text-mode REPL result events.
    ///
    /// Transformation:
    /// - Converts primitive and aggregate values to Terlan-facing syntax,
    ///   keeping `Unit`, `true`, and `false` distinct from backend atoms.
    pub(crate) fn render(&self) -> String {
        match self {
            Self::Unit => "Unit".to_string(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => value.clone(),
            Self::String(value) => format!("\"{}\"", escape_string(value)),
            Self::Atom(value) => format!("Atom[\"{}\"]", escape_string(value)),
            Self::Bool(value) => value.to_string(),
            Self::Type(value) => value.clone(),
            Self::Tuple(items) => {
                let rendered = items
                    .iter()
                    .map(Self::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{rendered}}}")
            }
            Self::List(items) => {
                let rendered = items
                    .iter()
                    .map(Self::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{rendered}]")
            }
            Self::Closure(_) => "<function>".to_string(),
        }
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
            .ok_or_else(|| format!("unknown REPL variable `{name}`")),
        CoreExpr::Tuple(items) => evaluate_exprs(core, items, env, output).map(ReplValue::Tuple),
        CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            evaluate_exprs(core, items, env, output).map(ReplValue::List)
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
        CoreExpr::Intrinsic(call) => evaluate_intrinsic(core, call, env, output),
        other => Err(format!(
            "CoreIR evaluator does not yet support {}",
            core_expr_kind(other)
        )),
    }
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
        other => Err(format!(
            "REPL evaluator does not yet support pattern {}",
            core_pattern_kind(other)
        )),
    }
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

/// Returns whether a name is an implicit target-neutral type value.
///
/// Inputs:
/// - `name`: source variable name encountered in CoreIR.
///
/// Output:
/// - `true` for type names admitted into the implicit prelude.
///
/// Transformation:
/// - Recognizes only compiler-backed type values. Standard-library algebraic
///   types and collections remain ordinary imports.
fn is_implicit_type_name(name: &str) -> bool {
    matches!(
        name,
        "Unit" | "Bool" | "Int" | "Float" | "String" | "Atom" | "Type"
    )
}

/// Computes the REPL-facing type value for an evaluated value.
///
/// Inputs:
/// - `value`: already evaluated REPL value.
///
/// Output:
/// - Source-facing type text such as `Int`, `String`, or `List[Int]`.
///
/// Transformation:
/// - Classifies runtime evaluator values without target-runtime reflection.
///   Aggregate types are rendered conservatively while the full language-level
///   type-value model is still being implemented.
fn type_of_value(value: &ReplValue) -> String {
    match value {
        ReplValue::Unit => "Unit".to_string(),
        ReplValue::Int(_) => "Int".to_string(),
        ReplValue::Float(_) => "Float".to_string(),
        ReplValue::String(_) => "String".to_string(),
        ReplValue::Atom(_) => "Atom".to_string(),
        ReplValue::Bool(_) => "Bool".to_string(),
        ReplValue::Type(_) => "Type".to_string(),
        ReplValue::Closure(_) => "Function".to_string(),
        ReplValue::Tuple(items) => {
            let types = items
                .iter()
                .map(type_of_value)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{types}}}")
        }
        ReplValue::List(items) => list_type_of_values(items),
    }
}

/// Computes the REPL-facing list type for evaluated list items.
///
/// Inputs:
/// - `items`: evaluated list elements.
///
/// Output:
/// - `List[T]` when all elements share a type, otherwise `List[Dynamic]`.
///
/// Transformation:
/// - Keeps list type rendering predictable without introducing union type
///   rendering into the REPL evaluator prematurely.
fn list_type_of_values(items: &[ReplValue]) -> String {
    let Some(first) = items.first() else {
        return "List[Dynamic]".to_string();
    };
    let first_type = type_of_value(first);
    if items.iter().all(|item| type_of_value(item) == first_type) {
        format!("List[{first_type}]")
    } else {
        "List[Dynamic]".to_string()
    }
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

/// Normalizes a CoreIR string payload into a runtime string value.
///
/// Inputs:
/// - `value`: CoreIR `Binary` payload.
///
/// Output:
/// - Runtime string without source quotes when quotes were preserved.
///
/// Transformation:
/// - Handles both raw and quoted payloads so the evaluator can tolerate the
///   transitional CoreIR string representation.
fn normalize_core_string(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value)
        .replace("\\\"", "\"")
        .replace("\\n", "\n")
}

/// Escapes a string for REPL source-style rendering.
///
/// Inputs:
/// - `value`: runtime string.
///
/// Output:
/// - Escaped string payload without surrounding quotes.
///
/// Transformation:
/// - Escapes the small set of characters needed by REPL display tests.
fn escape_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
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
#[path = "evaluator_test.rs"]
mod evaluator_test;
