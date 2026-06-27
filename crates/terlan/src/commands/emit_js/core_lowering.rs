use crate::terlan_typeck::{
    CoreCaseClause, CoreExpr, CoreFunction, CoreIfClause, CoreIntrinsicCall, CoreIntrinsicId,
    CoreModule, CorePattern, CorePrimitiveIntrinsic,
};

use super::cast_semantics::cast_can_lower_as_js_identity;

/// Serializes public CoreIR function signatures into minimal JS exports.
///
/// Inputs:
/// - `module`: backend-independent CoreIR module produced by the formal
///   compile pipeline.
///
/// Output:
/// - JavaScript source text containing exported function stubs or lowered
///   return bodies.
///
/// Transformation:
/// - Filters private CoreIR functions and emits one named `export function`
///   per public function signature. Functions in the supported expression
///   subset receive lowered return bodies; unsupported functions remain stubs.
pub(super) fn emit_core_module_to_js(module: &CoreModule) -> String {
    let mut out = String::new();
    for function in &module.functions {
        if !function.public {
            continue;
        }
        let params = function
            .params
            .iter()
            .map(|param| param.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "export function {}({}) {{\n",
            function.name, params
        ));
        if let Some(body) = emit_core_function_body_to_js(function) {
            out.push_str(&body);
        } else {
            out.push_str("  throw new Error(\"Terlan JS backend stub\");\n");
        }
        out.push_str("}\n\n");
    }
    out
}

/// Lowers one CoreIR function body into JavaScript when it is in the supported subset.
///
/// Inputs:
/// - `function`: one CoreIR function signature and clause set.
///
/// Output:
/// - JavaScript function body text for supported functions.
/// - `None` when clauses, guards, patterns, or expression forms are not yet
///   supported by the JS backend.
///
/// Transformation:
/// - Accepts only a single unguarded clause whose CoreIR patterns are direct
///   variable bindings matching the function parameters, then lowers the typed
///   CoreIR body expression into a `return` statement.
fn emit_core_function_body_to_js(function: &CoreFunction) -> Option<String> {
    let [clause] = function.clauses.as_slice() else {
        return None;
    };
    if clause.guard.is_some() {
        return None;
    }
    if !core_clause_patterns_match_function_params(function, &clause.core_patterns) {
        return None;
    }
    let body = core_expr_to_js(clause.body.core_expr.as_ref()?)?;
    Some(format!("  return {};\n", body))
}

/// Checks whether a CoreIR function clause uses direct parameter bindings.
///
/// Inputs:
/// - `function`: function whose declared params define the callable JS surface.
/// - `patterns`: CoreIR clause parameter patterns.
///
/// Output:
/// - `true` when each pattern is a variable with the same name as its
///   corresponding function parameter.
///
/// Transformation:
/// - Rejects destructuring, wildcard, literal, and unsupported patterns so the
///   first JS lowering slice does not need pattern dispatch.
fn core_clause_patterns_match_function_params(
    function: &CoreFunction,
    patterns: &[Option<CorePattern>],
) -> bool {
    function.params.len() == patterns.len()
        && function
            .params
            .iter()
            .zip(patterns.iter())
            .all(|(param, pattern)| {
                matches!(pattern, Some(CorePattern::Var(name)) if name == &param.name)
                    && is_js_identifier(&param.name)
            })
}

/// Lowers a supported CoreIR expression into JavaScript expression text.
///
/// Inputs:
/// - `expr`: typed CoreIR expression payload.
///
/// Output:
/// - JavaScript expression text for supported forms, or `None` for forms that
///   still require backend design work.
///
/// Transformation:
/// - Recursively lowers portable value, collection, unary, and binary CoreIR
///   expressions without reading source syntax summaries.
fn core_expr_to_js(expr: &CoreExpr) -> Option<String> {
    match expr {
        CoreExpr::Int(value) => Some(value.to_string()),
        CoreExpr::Float(value) => js_float_literal(value),
        CoreExpr::Atom(value) | CoreExpr::Var(value) if value == "true" || value == "false" => {
            Some(value.clone())
        }
        CoreExpr::Binary(value) | CoreExpr::Atom(value) => Some(js_string_literal(value)),
        CoreExpr::Var(name) if is_js_identifier(name) => Some(name.clone()),
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            let items = items
                .iter()
                .map(core_expr_to_js)
                .collect::<Option<Vec<_>>>()?
                .join(", ");
            Some(format!("[{}]", items))
        }
        CoreExpr::ListComprehension {
            expr,
            pattern,
            source,
            guard,
        } => core_list_comprehension_expr_to_js(expr, pattern, source, guard.as_deref()),
        CoreExpr::ListCons { head, tail } => Some(format!(
            "[{}, ...{}]",
            core_expr_to_js(head)?,
            core_expr_to_js(tail)?
        )),
        CoreExpr::Index { base, index } => Some(format!(
            "{}[{}]",
            core_expr_to_js(base)?,
            core_expr_to_js(index)?
        )),
        CoreExpr::FieldAccess { base, field } if is_js_identifier(field) => {
            Some(format!("{}.{}", core_expr_to_js(base)?, field))
        }
        CoreExpr::RecordAccess { base, field, .. } if is_js_identifier(field) => {
            Some(format!("{}.{}", core_expr_to_js(base)?, field))
        }
        CoreExpr::Map(fields) => core_map_expr_to_js(fields),
        CoreExpr::RecordConstruct { fields, .. } => core_record_expr_to_js(fields),
        CoreExpr::RecordUpdate { base, fields, .. } => core_record_update_expr_to_js(base, fields),
        CoreExpr::TemplateInstantiate { fields, .. } => core_template_expr_to_js(fields),
        CoreExpr::Case { scrutinee, clauses } => core_case_expr_to_js(scrutinee, clauses),
        CoreExpr::If { clauses } => core_if_expr_to_js(clauses),
        CoreExpr::Lam { params, body } => core_lam_expr_to_js(params, body),
        CoreExpr::Call { function, args } => core_call_expr_to_js(function, args),
        CoreExpr::FunctionCall { callee, args } => core_function_call_expr_to_js(callee, args),
        CoreExpr::Cast { expr, target_type } => {
            cast_can_lower_as_js_identity(expr, target_type).then(|| core_expr_to_js(expr))?
        }
        CoreExpr::Intrinsic(call) => core_intrinsic_call_expr_to_js(call),
        CoreExpr::UnaryOp { operator, operand } if operator == "-" => {
            Some(format!("(-{})", core_expr_to_js(operand)?))
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            if operator == "|>" {
                return core_pipe_forward_expr_to_js(left, right);
            }
            if operator == "div" {
                return core_integer_division_expr_to_js(left, right);
            }
            let operator = js_binary_operator(operator)?;
            Some(format!(
                "({} {} {})",
                core_expr_to_js(left)?,
                operator,
                core_expr_to_js(right)?
            ))
        }
        _ => None,
    }
}

/// Lowers a supported CoreIR intrinsic call into JavaScript expression text.
///
/// Inputs:
/// - `call`: CoreIR intrinsic call with a closed backend-neutral intrinsic id.
///
/// Output:
/// - JavaScript expression text for the supported intrinsic subset.
/// - `None` for intrinsic operations that are not yet selected for JS emission.
///
/// Transformation:
/// - Maps compiler-owned primitive intrinsic ids to JavaScript standard
///   operations while keeping JavaScript names out of CoreIR and source syntax.
fn core_intrinsic_call_expr_to_js(call: &CoreIntrinsicCall) -> Option<String> {
    match &call.id {
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringContains) => {
            let [value, pattern] = call.args.as_slice() else {
                return None;
            };
            Some(format!(
                "{}.includes({})",
                core_expr_to_js(value)?,
                core_expr_to_js(pattern)?
            ))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringStartsWith) => {
            let [value, prefix] = call.args.as_slice() else {
                return None;
            };
            Some(format!(
                "{}.startsWith({})",
                core_expr_to_js(value)?,
                core_expr_to_js(prefix)?
            ))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringLength) => {
            let [value] = call.args.as_slice() else {
                return None;
            };
            Some(format!("Array.from({}).length", core_expr_to_js(value)?))
        }
        _ => None,
    }
}

/// Lowers a local named CoreIR call into a JavaScript call expression.
///
/// Inputs:
/// - `function`: local CoreIR function name.
/// - `args`: CoreIR argument expressions.
///
/// Output:
/// - JavaScript call expression text for supported names and arguments.
/// - `None` when the function name is not a safe JavaScript identifier or any
///   argument remains outside the current JS subset.
///
/// Transformation:
/// - Emits `function(arg1, arg2, ...)` without introducing module or constructor
///   resolution semantics.
fn core_call_expr_to_js(function: &str, args: &[CoreExpr]) -> Option<String> {
    if !is_js_identifier(function) {
        return None;
    }
    let args = args
        .iter()
        .map(core_expr_to_js)
        .collect::<Option<Vec<_>>>()?
        .join(", ");
    Some(format!("{function}({args})"))
}

/// Lowers a CoreIR function-value invocation into JavaScript.
///
/// Inputs:
/// - `callee`: CoreIR expression producing a callable JavaScript value.
/// - `args`: CoreIR argument expressions.
///
/// Output:
/// - JavaScript call expression text for supported callable expressions and
///   arguments.
/// - `None` when the callee or any argument is outside the current JS subset.
///
/// Transformation:
/// - Emits `(callee)(arg1, arg2, ...)`, preserving Terlan's dedicated
///   `f.(args)` source semantics without reclassifying it as a named call.
fn core_function_call_expr_to_js(callee: &CoreExpr, args: &[CoreExpr]) -> Option<String> {
    let callee = core_expr_to_js(callee)?;
    let args = args
        .iter()
        .map(core_expr_to_js)
        .collect::<Option<Vec<_>>>()?
        .join(", ");
    Some(format!("({callee})({args})"))
}

/// Lowers a focused pipe-forward CoreIR expression into a local function call.
///
/// Inputs:
/// - `left`: CoreIR expression supplying the piped first argument.
/// - `right`: CoreIR expression expected to be a local named call or
///   function-value invocation.
///
/// Output:
/// - JavaScript call expression text for `left |> f(args...)`.
/// - `None` when the right side is not a local named call or any child
///   expression remains outside the current JS subset.
///
/// Transformation:
/// - Converts `left |> f(extra)` into `f(left, extra)` and `left |> f.(extra)`
///   into `(f)(left, extra)`.
fn core_pipe_forward_expr_to_js(left: &CoreExpr, right: &CoreExpr) -> Option<String> {
    match right {
        CoreExpr::Call { function, args } => {
            let mut piped_args = Vec::with_capacity(args.len() + 1);
            piped_args.push(core_expr_to_js(left)?);
            piped_args.extend(
                args.iter()
                    .map(core_expr_to_js)
                    .collect::<Option<Vec<_>>>()?,
            );
            if !is_js_identifier(function) {
                return None;
            }
            Some(format!("{function}({})", piped_args.join(", ")))
        }
        CoreExpr::FunctionCall { callee, args } => {
            let mut piped_args = Vec::with_capacity(args.len() + 1);
            piped_args.push(core_expr_to_js(left)?);
            piped_args.extend(
                args.iter()
                    .map(core_expr_to_js)
                    .collect::<Option<Vec<_>>>()?,
            );
            Some(format!(
                "({})({})",
                core_expr_to_js(callee)?,
                piped_args.join(", ")
            ))
        }
        _ => None,
    }
}

/// Lowers Terlan integer division into a JavaScript truncating division call.
///
/// Inputs:
/// - `left`: CoreIR dividend expression.
/// - `right`: CoreIR divisor expression.
///
/// Output:
/// - JavaScript expression text for supported child expressions.
/// - `None` when either child expression is outside the current JS subset.
///
/// Transformation:
/// - Emits `Math.trunc(left / right)` so Terlan `div` keeps integer quotient
///   semantics instead of becoming JavaScript floating-point `/`.
fn core_integer_division_expr_to_js(left: &CoreExpr, right: &CoreExpr) -> Option<String> {
    Some(format!(
        "Math.trunc({} / {})",
        core_expr_to_js(left)?,
        core_expr_to_js(right)?
    ))
}

/// Lowers a simple CoreIR list comprehension into a JavaScript `map` call.
///
/// Inputs:
/// - `expr`: yielded CoreIR expression.
/// - `pattern`: generator pattern bound for each source element.
/// - `source`: CoreIR source-list expression.
/// - `guard`: optional CoreIR guard expression.
///
/// Output:
/// - JavaScript expression text for a single-generator, variable-pattern,
///   unguarded list comprehension.
/// - `None` for guarded comprehensions, destructuring patterns, unsupported
///   parameter names, or unsupported yield/source expressions.
///
/// Transformation:
/// - Converts `[yield | value <- source]` into
///   `(source).map((value) => yield)`. This preserves the current list-valued
///   artifact shape without introducing filter semantics or pattern dispatch.
fn core_list_comprehension_expr_to_js(
    expr: &CoreExpr,
    pattern: &CorePattern,
    source: &CoreExpr,
    guard: Option<&CoreExpr>,
) -> Option<String> {
    if guard.is_some() {
        return None;
    }
    let param = core_lam_param_to_js(pattern)?;
    Some(format!(
        "({}).map(({param}) => {})",
        core_expr_to_js(source)?,
        core_expr_to_js(expr)?
    ))
}

/// Lowers a CoreIR anonymous function value into a JavaScript arrow function.
///
/// Inputs:
/// - `params`: CoreIR lambda parameter patterns.
/// - `body`: CoreIR lambda body expression.
///
/// Output:
/// - JavaScript arrow-function expression text when every parameter is a direct
///   variable binding and the body expression is lowerable.
/// - `None` for destructuring parameters, wildcard parameters, unsupported
///   parameter names, or unsupported body expressions.
///
/// Transformation:
/// - Converts Terlan `(patterns) -> Expr` lambda values into parenthesized
///   JavaScript arrow functions. This only lowers the function value;
///   callable-value invocation is handled by the dedicated `f.(args)` syntax.
fn core_lam_expr_to_js(params: &[CorePattern], body: &CoreExpr) -> Option<String> {
    let params = params
        .iter()
        .map(core_lam_param_to_js)
        .collect::<Option<Vec<_>>>()?
        .join(", ");
    Some(format!("(({params}) => {})", core_expr_to_js(body)?))
}

/// Converts one CoreIR lambda parameter pattern into a JavaScript parameter name.
///
/// Inputs:
/// - `param`: CoreIR lambda parameter pattern.
///
/// Output:
/// - JavaScript identifier text for direct variable patterns.
/// - `None` for every non-variable pattern or unsupported identifier.
///
/// Transformation:
/// - Keeps this JS slice limited to non-destructuring anonymous functions and
///   reuses the backend's conservative JavaScript identifier policy.
fn core_lam_param_to_js(param: &CorePattern) -> Option<String> {
    let CorePattern::Var(name) = param else {
        return None;
    };
    is_js_identifier(name).then(|| name.clone())
}

/// Lowers a total literal-pattern CoreIR case expression into JavaScript.
///
/// Inputs:
/// - `scrutinee`: CoreIR expression being matched.
/// - `clauses`: CoreIR case clauses in source order.
///
/// Output:
/// - JavaScript conditional expression text for cases whose scrutinee is a
///   direct variable, whose non-final clauses are unguarded atom, integer, or
///   finite-float literals, and whose final clause is an unguarded wildcard
///   fallback.
/// - `None` for partial cases, guarded cases, binding patterns, destructuring
///   patterns, or unsupported branch bodies.
///
/// Transformation:
/// - Treats the final wildcard clause as the JavaScript alternate branch and
///   folds preceding literal clauses from right to left into nested ternary
///   expressions. The scrutinee is restricted to a variable so this bootstrap
///   text path does not introduce a repeated-evaluation policy.
fn core_case_expr_to_js(scrutinee: &CoreExpr, clauses: &[CoreCaseClause]) -> Option<String> {
    let CoreExpr::Var(scrutinee_name) = scrutinee else {
        return None;
    };
    if !is_js_identifier(scrutinee_name) {
        return None;
    }

    let (fallback, clauses) = clauses.split_last()?;
    if fallback.guard.is_some() || !matches!(fallback.pattern, CorePattern::Wildcard) {
        return None;
    }

    let mut expr = core_expr_to_js(&fallback.body)?;
    for clause in clauses.iter().rev() {
        if clause.guard.is_some() {
            return None;
        }
        let condition = core_case_literal_pattern_condition_to_js(scrutinee_name, &clause.pattern)?;
        expr = format!(
            "({condition} ? {} : {expr})",
            core_expr_to_js(&clause.body)?
        );
    }
    Some(expr)
}

/// Renders a JavaScript equality test for one supported CoreIR case pattern.
///
/// Inputs:
/// - `scrutinee_name`: already-validated JavaScript identifier holding the case
///   scrutinee value.
/// - `pattern`: CoreIR pattern from a non-final case clause.
///
/// Output:
/// - JavaScript strict-equality expression text for atom, integer, and
///   finite-float literal patterns.
/// - `None` for every other pattern shape.
///
/// Transformation:
/// - Reuses CoreIR atom artifact lowering, so boolean atoms compare against
///   JavaScript booleans and other atoms compare against string-like artifacts.
///   Numeric patterns compare against JavaScript numeric literals.
fn core_case_literal_pattern_condition_to_js(
    scrutinee_name: &str,
    pattern: &CorePattern,
) -> Option<String> {
    match pattern {
        CorePattern::Atom(value) => {
            let atom = core_expr_to_js(&CoreExpr::Atom(value.clone()))?;
            Some(format!("{scrutinee_name} === {atom}"))
        }
        CorePattern::Int(value) => Some(format!("{scrutinee_name} === {value}")),
        CorePattern::Float(value) => {
            let float = js_float_literal(value)?;
            Some(format!("{scrutinee_name} === {float}"))
        }
        _ => None,
    }
}

/// Maps a supported CoreIR binary operator to JavaScript.
///
/// Inputs:
/// - `operator`: CoreIR operator spelling.
///
/// Output:
/// - JavaScript operator spelling, or `None` when unsupported.
///
/// Transformation:
/// - Preserves arithmetic/comparison operators and lowers Terlan equality to
///   JavaScript strict equality.
fn js_binary_operator(operator: &str) -> Option<&'static str> {
    match operator {
        "+" => Some("+"),
        "-" => Some("-"),
        "*" => Some("*"),
        "/" => Some("/"),
        "rem" => Some("%"),
        "==" => Some("==="),
        "=:=" => Some("==="),
        "!=" | "/=" => Some("!=="),
        "=/=" => Some("!=="),
        "<" => Some("<"),
        "<=" => Some("<="),
        ">" => Some(">"),
        ">=" => Some(">="),
        "and" | "&&" => Some("&&"),
        "or" | "||" => Some("||"),
        _ => None,
    }
}

/// Lowers a total CoreIR if expression into a JavaScript conditional expression.
///
/// Inputs:
/// - `clauses`: CoreIR if clauses in source order.
///
/// Output:
/// - JavaScript conditional expression text for if expressions whose final
///   clause is literal `true`.
/// - `None` when the expression is empty, has no final fallback, or contains
///   unsupported condition/body expressions.
///
/// Transformation:
/// - Treats the final `true -> body` clause as the JavaScript alternate branch
///   and folds preceding clauses from right to left into nested ternary
///   expressions. Single-clause `if` without an explicit fallback stays
///   unsupported because Terlan no-match runtime behavior is not represented
///   in the current JS artifact subset.
fn core_if_expr_to_js(clauses: &[CoreIfClause]) -> Option<String> {
    let (fallback, clauses) = clauses.split_last()?;
    if !core_expr_is_true_literal(&fallback.condition) {
        return None;
    }

    let mut expr = core_expr_to_js(&fallback.body)?;
    for clause in clauses.iter().rev() {
        expr = format!(
            "({} ? {} : {})",
            core_expr_to_js(&clause.condition)?,
            core_expr_to_js(&clause.body)?,
            expr
        );
    }
    Some(expr)
}

/// Checks whether a CoreIR expression is the boolean `true` literal.
///
/// Inputs:
/// - `expr`: CoreIR expression to classify.
///
/// Output:
/// - `true` when the expression is `CoreExpr::Atom("true")` or
///   `CoreExpr::Var("true")`.
///
/// Transformation:
/// - Recognizes the CoreIR representation currently produced for Terlan
///   boolean `true`, without treating arbitrary atom values as booleans.
fn core_expr_is_true_literal(expr: &CoreExpr) -> bool {
    matches!(expr, CoreExpr::Atom(value) | CoreExpr::Var(value) if value == "true")
}

/// Renders a CoreIR float literal as a JavaScript numeric literal.
///
/// Inputs:
/// - `value`: float payload text captured in CoreIR.
///
/// Output:
/// - JavaScript numeric literal text for finite values.
/// - `None` when the payload does not parse as a finite JavaScript-compatible
///   number.
///
/// Transformation:
/// - Validates the CoreIR payload through Rust's `f64` parser and preserves the
///   original decimal spelling for output so bootstrap lowering does not
///   silently introduce a new formatting policy.
fn js_float_literal(value: &str) -> Option<String> {
    value
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
        .map(|_| value.to_string())
}

/// Lowers CoreIR map fields into a JavaScript object literal.
///
/// Inputs:
/// - `fields`: structured CoreIR map fields from syntax-output lowering.
///
/// Output:
/// - JavaScript object literal text for identifier-key fields whose values are
///   in the supported expression subset.
/// - `None` when a field key cannot be emitted as a plain JavaScript property
///   name or a value remains unsupported.
///
/// Transformation:
/// - Treats the current map-literal JS subset as a plain object with static
///   identifier keys, preserving field order and recursively lowering values.
///   The Terlan `=>`/`:=` distinction is intentionally not represented in this
///   construction-only object literal subset.
fn core_map_expr_to_js(fields: &[crate::terlan_typeck::CoreMapExprField]) -> Option<String> {
    let fields = fields
        .iter()
        .map(|field| {
            if !is_js_identifier(&field.key) {
                return None;
            }
            Some(format!("{}: {}", field.key, core_expr_to_js(&field.value)?))
        })
        .collect::<Option<Vec<_>>>()?
        .join(", ");
    Some(format!("{{{fields}}}"))
}

/// Lowers CoreIR record-construction fields into a JavaScript object literal.
///
/// Inputs:
/// - `fields`: structured CoreIR record fields from syntax-output lowering.
///
/// Output:
/// - JavaScript object literal text for identifier-key fields whose values are
///   in the supported expression subset.
/// - `None` when a field key cannot be emitted as a plain JavaScript property
///   name or a value remains unsupported.
///
/// Transformation:
/// - Uses the current JS backend struct representation: a plain object whose
///   field names match Terlan struct fields. The CoreIR record name is handled
///   by the caller and intentionally not encoded in the v0 JS object value.
fn core_record_expr_to_js(fields: &[crate::terlan_typeck::CoreRecordExprField]) -> Option<String> {
    let fields = fields
        .iter()
        .map(|field| {
            if !is_js_identifier(&field.key) {
                return None;
            }
            Some(format!("{}: {}", field.key, core_expr_to_js(&field.value)?))
        })
        .collect::<Option<Vec<_>>>()?
        .join(", ");
    Some(format!("{{{fields}}}"))
}

/// Lowers a CoreIR record update into a JavaScript object spread expression.
///
/// Inputs:
/// - `base`: receiver object being updated.
/// - `fields`: structured CoreIR record fields from syntax-output lowering.
///
/// Output:
/// - JavaScript object-spread expression for identifier-key fields whose base
///   and values are in the supported expression subset.
/// - `None` when the base, a field key, or a field value remains unsupported.
///
/// Transformation:
/// - Uses the current JS backend struct-update representation:
///   `({...base, field: value})`. This preserves functional update behavior for
///   plain object struct values without mutating the receiver.
fn core_record_update_expr_to_js(
    base: &CoreExpr,
    fields: &[crate::terlan_typeck::CoreRecordExprField],
) -> Option<String> {
    let fields = fields
        .iter()
        .map(|field| {
            if !is_js_identifier(&field.key) {
                return None;
            }
            Some(format!("{}: {}", field.key, core_expr_to_js(&field.value)?))
        })
        .collect::<Option<Vec<_>>>()?;
    let mut parts = vec![format!("...{}", core_expr_to_js(base)?)];
    parts.extend(fields);
    Some(format!("({{{}}})", parts.join(", ")))
}

/// Lowers CoreIR template-instantiation props into a JavaScript object literal.
///
/// Inputs:
/// - `fields`: structured CoreIR template prop assignments from syntax-output
///   lowering.
///
/// Output:
/// - JavaScript object literal text for identifier-key props whose values are
///   in the supported expression subset.
/// - `None` when a prop key cannot be emitted as a plain JavaScript property
///   name or a value remains unsupported.
///
/// Transformation:
/// - Uses the current JS backend artifact representation for template values:
///   a plain object whose property names match Terlan template props. This
///   deliberately does not render HTML; static template rendering stays in the
///   static-site command path.
fn core_template_expr_to_js(
    fields: &[crate::terlan_typeck::CoreRecordExprField],
) -> Option<String> {
    core_record_expr_to_js(fields)
}

/// Checks whether text is safe to emit as a JavaScript identifier.
///
/// Inputs:
/// - `name`: CoreIR variable or parameter name.
///
/// Output:
/// - `true` when the name can be emitted as an identifier without quoting.
///
/// Transformation:
/// - Applies a conservative ASCII identifier subset and leaves keyword
///   avoidance to later backend hardening.
fn is_js_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
}

/// Renders a JavaScript string literal.
///
/// Inputs:
/// - `value`: atom payload or CoreIR binary payload, which may still include
///   source-level surrounding quotes.
///
/// Output:
/// - Double-quoted JavaScript string literal text.
///
/// Transformation:
/// - Normalizes quoted CoreIR binary payloads to their runtime string value,
///   then escapes backslashes, double quotes, and common control characters.
fn js_string_literal(value: &str) -> String {
    let value = core_string_runtime_value(value);
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Normalizes a CoreIR string-like payload into a runtime string value.
///
/// Inputs:
/// - `value`: CoreIR string-like payload from `CoreExpr::Binary` or
///   `CoreExpr::Atom`.
///
/// Output:
/// - Runtime string content for quoted source literals, or the original payload
///   for atom/unquoted values.
///
/// Transformation:
/// - Parses JSON-compatible quoted source string payloads so JavaScript output
///   does not preserve Terlan source delimiters as runtime characters.
fn core_string_runtime_value(value: &str) -> String {
    if value.starts_with('"') && value.ends_with('"') {
        serde_json::from_str::<String>(value)
            .unwrap_or_else(|_| value.trim_matches('"').to_string())
    } else {
        value.to_string()
    }
}
