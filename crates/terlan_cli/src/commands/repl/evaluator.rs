use std::collections::HashMap;

use terlan_typeck::{
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
                next_env.insert(binding.name.clone(), value);
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
        CoreExpr::Call { function, args } => evaluate_call(core, function, args, env, output),
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

/// Returns whether a name is an implicit target-neutral type value.
///
/// Inputs:
/// - `name`: source variable name encountered in CoreIR.
///
/// Output:
/// - `true` for type names admitted into the 0.0.3 implicit prelude.
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
    call: &terlan_typeck::CoreIntrinsicCall,
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
/// - Implements the only required 0.0.3 REPL effect hook while keeping target
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
        CoreExpr::Intrinsic(_) => "Intrinsic",
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
mod tests {
    use super::*;
    use crate::validation::native_policy::NativePolicy;
    use crate::validation::target_profile::TargetProfile;
    use crate::{ColorChoice, DiagnosticFormat};

    /// Verifies simple arithmetic source evaluates without backend execution.
    ///
    /// Inputs:
    /// - Source module with one zero-arity function body `1 + 2`.
    ///
    /// Output:
    /// - Test assertion only; no files or target runtimes are touched.
    ///
    /// Transformation:
    /// - Compiles through the formal pipeline, then runs the compiler-owned
    ///   evaluator and asserts Terlan-facing value rendering.
    #[test]
    fn evaluator_renders_simple_integer_expression() {
        let core = compile_core("module repl_test.\n\npub run(): Dynamic ->\n    1 + 2.\n");

        let value = evaluate_repl_function(&core, "run").expect("evaluate");

        assert_eq!(value.render(), "3");
    }

    /// Verifies console output returns `Unit` through the evaluator hook.
    ///
    /// Inputs:
    /// - Source module importing `println` and calling it.
    ///
    /// Output:
    /// - Test assertion for the returned value.
    ///
    /// Transformation:
    /// - Compiles through the formal pipeline, executes the selected std effect
    ///   hook through the compiler-owned evaluator, and checks it still returns
    ///   the Terlan `Unit` value.
    #[test]
    fn evaluator_returns_unit_for_console_println() {
        let core = compile_core(
            "module repl_test.\n\nimport std.io.Console.{println}.\n\npub run(): Unit ->\n    println(\"hello\").\n",
        );

        let value = evaluate_repl_function(&core, "run").expect("evaluate");

        assert_eq!(value, ReplValue::Unit);
    }

    /// Verifies console output can be captured by the caller.
    ///
    /// Inputs:
    /// - Source module importing `println` and calling it once.
    /// - Caller-owned output buffer.
    ///
    /// Output:
    /// - Test assertion for returned `Unit` and captured output payload.
    ///
    /// Transformation:
    /// - Executes the same evaluator hook through the output-aware entry point
    ///   used by JSON REPL mode instead of printing directly from the evaluator.
    #[test]
    fn evaluator_routes_console_println_through_output_sink() {
        let core = compile_core(
            "module repl_test.\n\nimport std.io.Console.{println}.\n\npub run(): Unit ->\n    println(\"hello\").\n",
        );
        let mut output = Vec::new();
        let mut capture = |value: &str| output.push(value.to_string());

        let value =
            evaluate_repl_function_with_output(&core, "run", &mut capture).expect("evaluate");

        assert_eq!(value, ReplValue::Unit);
        assert_eq!(output, vec!["hello".to_string()]);
    }

    /// Verifies source-level `type_of` returns a REPL `Type` value.
    ///
    /// Inputs:
    /// - Source module with one zero-arity function body `type_of(1)`.
    ///
    /// Output:
    /// - Test assertion for rendered type syntax.
    ///
    /// Transformation:
    /// - Compiles the implicit function call through the formal path, then
    ///   executes the compiler-backed REPL type introspection hook.
    #[test]
    fn evaluator_supports_type_of_for_integer() {
        let core = compile_core("module repl_test.\n\npub run(): Dynamic ->\n    type_of(1).\n");

        let value = evaluate_repl_function(&core, "run").expect("evaluate");

        assert_eq!(value, ReplValue::Type("Int".to_string()));
        assert_eq!(value.render(), "Int");
    }

    /// Verifies source-level `is_type` compares against implicit type values.
    ///
    /// Inputs:
    /// - Source module with one zero-arity function body `is_type(1, Int)`.
    ///
    /// Output:
    /// - Test assertion for a boolean result.
    ///
    /// Transformation:
    /// - Treats `Int` as an implicit type value in expression position and
    ///   compares it to the evaluated first argument's type.
    #[test]
    fn evaluator_supports_is_type_for_implicit_type_value() {
        let core =
            compile_core("module repl_test.\n\npub run(): Dynamic ->\n    is_type(1, Int).\n");

        let value = evaluate_repl_function(&core, "run").expect("evaluate");

        assert_eq!(value, ReplValue::Bool(true));
        assert_eq!(value.render(), "true");
    }

    /// Compiles a test module into CoreIR for evaluator assertions.
    ///
    /// Inputs:
    /// - `source`: complete Terlan source module.
    ///
    /// Output:
    /// - CoreIR module produced by the formal compiler pipeline.
    ///
    /// Transformation:
    /// - Reuses the production formal pipeline so evaluator tests exercise the
    ///   same CoreIR payloads the REPL receives.
    fn compile_core(source: &str) -> CoreModule {
        crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
            "<repl-evaluator-test>.terl",
            source,
            DiagnosticFormat::Text {
                color: ColorChoice::Never,
            },
            None,
            NativePolicy::SafeNativeOptional,
            TargetProfile::Erlang,
        )
        .expect("compile evaluator source")
        .core
    }
}
