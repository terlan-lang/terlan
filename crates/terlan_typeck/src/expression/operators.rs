use super::*;

/// Infers a binary operator expression.
///
/// Inputs:
/// - `expr`: syntax-output binary operator expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - Operator result type, or `Dynamic` for malformed operators.
///
/// Transformation:
/// - Infers both operands, parses the operator token, handles pipe forwarding,
///   and delegates ordinary operators to binary type rules.
pub(super) fn infer_syntax_binary_op(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let op = syntax_binary_op(expr.operator.as_deref());
    if matches!(op, SyntaxBinaryOp::PipeForward) {
        return infer_syntax_pipe_forward(expr, locals, ctx, subst, errors);
    }
    let left_type = expr
        .children
        .first()
        .map(|left| infer_syntax_expr(left, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let right_type = expr
        .children
        .get(1)
        .map(|right| infer_syntax_expr(right, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    infer_syntax_binary_types(&op, &left_type, &right_type, ctx.aliases, subst, errors)
}

/// Infers a unary operator expression.
///
/// Inputs:
/// - `expr`: syntax-output unary operator expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - Unary operator result type.
///
/// Transformation:
/// - Infers the operand, then applies numeric or boolean unary constraints.
pub(super) fn infer_syntax_unary_op(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let inner_type = expr
        .children
        .first()
        .map(|inner| infer_syntax_expr(inner, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    infer_unary_operator(
        expr.operator.as_deref().unwrap_or(""),
        &inner_type,
        subst,
        errors,
    )
}

/// Applies unary operator type rules.
///
/// Inputs:
/// - `op`: unary operator token.
/// - `value`: inferred operand type.
/// - `subst` and `errors`: active substitution and diagnostic state.
///
/// Output:
/// - Operator result type.
///
/// Transformation:
/// - Constrains numeric negation to numbers and logical negation to booleans.
fn infer_unary_operator(
    op: &str,
    value: &Type,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    match op {
        "-" => {
            if let Err(message) = unify(value, &Type::Number, subst) {
                errors.push(message);
            }
            let normalized = apply_subst(value, subst);
            if is_int_like(&normalized) {
                Type::Int
            } else if matches!(normalized, Type::Float) {
                Type::Float
            } else {
                Type::Number
            }
        }
        "not" | "!" => {
            if let Err(message) = unify(value, &Type::Bool, subst) {
                errors.push(message);
            }
            Type::Bool
        }
        _ => Type::Dynamic,
    }
}

/// Applies binary operator type rules.
///
/// Inputs:
/// - `op`: parsed binary operator.
/// - `left` and `right`: inferred operand types.
/// - `aliases`, `subst`, and `errors`: alias, substitution, and diagnostic
///   state.
///
/// Output:
/// - Operator result type.
///
/// Transformation:
/// - Enforces arithmetic, comparison, boolean, and division constraints and
///   returns the source-level operator result type.
fn infer_syntax_binary_types(
    op: &SyntaxBinaryOp,
    left: &Type,
    right: &Type,
    aliases: &HashMap<String, TypeAlias>,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    match op {
        SyntaxBinaryOp::Add if is_string_concat_pair(left, right) => Type::Binary,
        SyntaxBinaryOp::Add | SyntaxBinaryOp::Sub | SyntaxBinaryOp::Mul => {
            if let Err(message) = unify(left, &Type::Number, subst) {
                errors.push(format!("left side {}", message));
            }
            if let Err(message) = unify(right, &Type::Number, subst) {
                errors.push(format!("right side {}", message));
            }

            let normalized_left = apply_subst(left, subst);
            let normalized_right = apply_subst(right, subst);
            if is_int_like(&normalized_left) && is_int_like(&normalized_right) {
                Type::Int
            } else {
                Type::Number
            }
        }
        SyntaxBinaryOp::Div => {
            if let Err(message) = unify(left, &Type::Number, subst) {
                errors.push(format!("left side {}", message));
            }
            if let Err(message) = unify(right, &Type::Number, subst) {
                errors.push(format!("right side {}", message));
            }

            let normalized_left = apply_subst(left, subst);
            let normalized_right = apply_subst(right, subst);
            if is_int_like(&normalized_left) && is_int_like(&normalized_right) {
                Type::Int
            } else {
                Type::Number
            }
        }
        SyntaxBinaryOp::DivRem => {
            if let Err(message) = unify(left, &Type::Int, subst) {
                errors.push(format!("left side {}", message));
            }
            if let Err(message) = unify(right, &Type::Int, subst) {
                errors.push(format!("right side {}", message));
            }
            Type::Int
        }
        SyntaxBinaryOp::Eq
        | SyntaxBinaryOp::EqEq
        | SyntaxBinaryOp::EqEqEq
        | SyntaxBinaryOp::NotEq
        | SyntaxBinaryOp::NotEqEq
        | SyntaxBinaryOp::Lt
        | SyntaxBinaryOp::Gt
        | SyntaxBinaryOp::LtEq
        | SyntaxBinaryOp::GtEq => {
            if let Err(message) = unify_comparable_types(left, right, aliases, subst) {
                errors.push(message);
            }
            Type::Bool
        }
        SyntaxBinaryOp::And | SyntaxBinaryOp::Or => {
            if let Err(message) = unify(&Type::Bool, left, subst) {
                errors.push(format!("left side {}", message));
            }
            if let Err(message) = unify(&Type::Bool, right, subst) {
                errors.push(format!("right side {}", message));
            }
            Type::Bool
        }
        SyntaxBinaryOp::PipeForward => Type::Dynamic,
    }
}

/// Returns whether a type is the Terlan string/text representation.
///
/// Inputs:
/// - `ty`: inferred operand type.
///
/// Output:
/// - `true` for string literals and named `String` values.
///
/// Transformation:
/// - Treats the current compiler-internal `Binary` string representation and
///   explicit `String` type references as the same source-level string
///   surface for `Add[String, String, String]`.
fn is_string_like(ty: &Type) -> bool {
    matches!(ty, Type::Binary)
        || matches!(
            ty,
            Type::Named {
                name,
                ..
            } if name == "String"
        )
}

/// Returns whether two operands form a string concatenation pair.
///
/// Inputs:
/// - `left` and `right`: inferred operand types for `+`.
///
/// Output:
/// - `true` when either side is string-like and the other side is string-like
///   or a printable scalar.
///
/// Transformation:
/// - Keeps numeric `+` separate from display-oriented concatenation while
///   allowing common `"prefix" + value` print-path code without explicit
///   `to_string` calls.
fn is_string_concat_pair(left: &Type, right: &Type) -> bool {
    (is_string_like(left) && is_string_concat_operand(right))
        || (is_string_like(right) && is_string_concat_operand(left))
}

/// Returns whether a type can participate in string concatenation.
///
/// Inputs:
/// - `ty`: inferred operand type.
///
/// Output:
/// - `true` for strings and scalar values with stable textual rendering.
///
/// Transformation:
/// - Limits implicit display conversion to primitive scalar shapes so records,
///   collections, and functions still require explicit formatting APIs.
fn is_string_concat_operand(ty: &Type) -> bool {
    is_string_like(ty)
        || matches!(
            ty,
            Type::Int
                | Type::Float
                | Type::Number
                | Type::Bool
                | Type::Atom
                | Type::LiteralAtom(_)
                | Type::LiteralInt(_)
        )
}

/// Unifies binary comparison operands with transparent alias expansion.
///
/// Inputs:
/// - `left` and `right`: inferred operand types from a comparison expression.
/// - `aliases`: the visible type aliases for the current inference context.
/// - `subst`: the mutable type-variable substitution table.
///
/// Output:
/// - `Ok(())` when the operands are directly compatible, or compatible after
///   transparent alias expansion.
/// - The original direct-unification diagnostic when expansion still fails.
///
/// Transformation:
/// - First attempts normal unification so existing substitutions and
///   diagnostics remain unchanged for ordinary comparisons.
/// - If that fails, expands non-opaque aliases on both sides and retries using
///   the same substitution table.
fn unify_comparable_types(
    left: &Type,
    right: &Type,
    aliases: &HashMap<String, TypeAlias>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    if let Err(original_message) = unify(left, right, subst) {
        let left_expanded = expand_type_aliases(left, aliases);
        let right_expanded = expand_type_aliases(right, aliases);
        if unify(&left_expanded, &right_expanded, subst).is_err() {
            return Err(original_message);
        }
    }

    Ok(())
}

/// Checks whether a type can act as an integer.
///
/// Inputs:
/// - `ty`: inferred type.
///
/// Output:
/// - `true` for `Int` and integer literals.
///
/// Transformation:
/// - Performs a small structural match without alias expansion.
fn is_int_like(ty: &Type) -> bool {
    matches!(ty, Type::Int | Type::LiteralInt(_))
}

/// Binary operators recognized by expression type inference.
///
/// Inputs:
/// - Constructed from syntax-output operator text by `syntax_binary_op`.
///
/// Output:
/// - Internal operator category used to select type inference rules.
///
/// Transformation:
/// - Groups spelling variants such as `!=` and `/=` into one semantic branch
///   while preserving compatibility branches that still need diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyntaxBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    EqEq,
    EqEqEq,
    NotEq,
    NotEqEq,
    GtEq,
    Lt,
    Gt,
    LtEq,
    DivRem,
    And,
    Or,
    PipeForward,
}

/// Converts syntax-output operator text into an internal operator category.
///
/// Inputs:
/// - `operator`: optional operator spelling from a syntax-output binary
///   expression.
///
/// Output:
/// - Internal binary operator category.
///
/// Transformation:
/// - Maps supported arithmetic, comparison, boolean, and pipe spellings to the
///   inference enum, defaulting to assignment-style equality for missing or
///   unknown spellings to preserve existing recovery behavior.
fn syntax_binary_op(operator: Option<&str>) -> SyntaxBinaryOp {
    match operator.unwrap_or("=") {
        "+" => SyntaxBinaryOp::Add,
        "-" => SyntaxBinaryOp::Sub,
        "*" => SyntaxBinaryOp::Mul,
        "/" => SyntaxBinaryOp::Div,
        "=" => SyntaxBinaryOp::Eq,
        "==" => SyntaxBinaryOp::EqEq,
        "=:=" => SyntaxBinaryOp::EqEqEq,
        "!=" | "/=" => SyntaxBinaryOp::NotEq,
        "=/=" => SyntaxBinaryOp::NotEqEq,
        ">=" => SyntaxBinaryOp::GtEq,
        "<" => SyntaxBinaryOp::Lt,
        ">" => SyntaxBinaryOp::Gt,
        "<=" => SyntaxBinaryOp::LtEq,
        "div" | "rem" => SyntaxBinaryOp::DivRem,
        "and" | "&&" => SyntaxBinaryOp::And,
        "or" | "||" => SyntaxBinaryOp::Or,
        "|>" => SyntaxBinaryOp::PipeForward,
        _ => SyntaxBinaryOp::Eq,
    }
}
