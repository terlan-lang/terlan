/// Checks whether an integer can be represented exactly as a JavaScript number.
///
/// Inputs:
/// - `value`: CoreIR integer value under consideration for direct JS numeric
///   literal emission.
///
/// Output:
/// - `true` when the integer is within JavaScript's exact safe-integer range.
///
/// Transformation:
/// - Compares the integer to the ECMAScript `Number.MAX_SAFE_INTEGER` bound so
///   wider integer handling can fall back until the backend has a deliberate
///   bigint or runtime-number policy.
pub(super) fn is_js_safe_integer(value: i64) -> bool {
    value <= 9_007_199_254_740_991
}

/// Converts a CoreIR float payload into an Oxc numeric literal value.
///
/// Inputs:
/// - `value`: float payload text captured in CoreIR.
///
/// Output:
/// - Finite `f64` value for Oxc numeric-literal construction.
/// - `None` when the payload cannot be represented as a finite JavaScript
///   number.
///
/// Transformation:
/// - Parses the canonical CoreIR float payload and rejects infinities/NaN so
///   the direct backend does not invent target-specific runtime-number policy.
pub(super) fn core_float_literal_to_oxc_number(value: &str) -> Option<f64> {
    value
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

/// Copies a source identifier name into Oxc's AST arena.
///
/// Inputs:
/// - `ast`: Oxc AST builder that owns the destination allocator.
/// - `name`: CoreIR identifier text borrowed from compiler data structures.
///
/// Output:
/// - Identifier text with the same arena lifetime as the Oxc AST being built.
///
/// Transformation:
/// - Allocates the identifier bytes in Oxc's arena so generated AST nodes do
///   not borrow from the shorter-lived CoreIR module.
pub(super) fn oxc_ident_name<'a>(ast: oxc_ast::AstBuilder<'a>, name: &str) -> &'a str {
    ast.allocator.alloc_str(name)
}

/// Copies a runtime string literal value into Oxc's AST arena.
///
/// Inputs:
/// - `ast`: Oxc AST builder that owns the destination allocator.
/// - `value`: CoreIR string-like literal payload, which may still include
///   source-level surrounding quotes.
///
/// Output:
/// - String value with the same arena lifetime as the Oxc AST being built.
///
/// Transformation:
/// - Normalizes quoted CoreIR binary payloads to runtime string content, then
///   allocates the literal payload in Oxc's arena before creating a
///   `StringLiteral` node.
pub(super) fn oxc_string_value<'a>(ast: oxc_ast::AstBuilder<'a>, value: &str) -> &'a str {
    let value = core_string_runtime_value(value);
    ast.allocator.alloc_str(value.as_str())
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
/// - Parses JSON-compatible quoted source string payloads so Oxc string
///   literals do not preserve Terlan source delimiters as runtime characters.
fn core_string_runtime_value(value: &str) -> String {
    if value.starts_with('"') && value.ends_with('"') {
        serde_json::from_str::<String>(value)
            .unwrap_or_else(|_| value.trim_matches('"').to_string())
    } else {
        value.to_string()
    }
}

/// Maps a CoreIR unary operator to an Oxc unary operator.
///
/// Inputs:
/// - `operator`: CoreIR operator spelling.
///
/// Output:
/// - Oxc unary operator for the supported direct backend subset, or `None`
///   otherwise.
///
/// Transformation:
/// - Preserves Terlan unary minus as JavaScript unary negation and rejects all
///   other unary spellings until their semantics are selected explicitly.
pub(super) fn core_unary_operator_to_oxc(
    operator: &str,
) -> Option<oxc_syntax::operator::UnaryOperator> {
    use oxc_syntax::operator::UnaryOperator;

    match operator {
        "-" => Some(UnaryOperator::UnaryNegation),
        _ => None,
    }
}

/// Checks whether a CoreIR name is safe for direct Oxc identifier emission.
///
/// Inputs:
/// - `name`: function, parameter, or variable name from CoreIR.
///
/// Output:
/// - `true` when the name can be represented as a JavaScript identifier in the
///   current direct-AST subset.
///
/// Transformation:
/// - Applies the same conservative ASCII identifier subset used by bootstrap
///   string lowering so unsupported names can fall back instead of producing a
///   malformed JavaScript AST.
pub(super) fn is_direct_oxc_js_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
}

/// Maps a CoreIR binary operator to an Oxc binary operator.
///
/// Inputs:
/// - `operator`: CoreIR operator spelling.
///
/// Output:
/// - Oxc binary operator for the supported smoke subset, or `None` otherwise.
///
/// Transformation:
/// - Converts Terlan equality to JavaScript strict equality and preserves
///   arithmetic/comparison operators with equivalent JavaScript semantics.
pub(super) fn core_binary_operator_to_oxc(
    operator: &str,
) -> Option<oxc_syntax::operator::BinaryOperator> {
    use oxc_syntax::operator::BinaryOperator;

    match operator {
        "+" => Some(BinaryOperator::Addition),
        "-" => Some(BinaryOperator::Subtraction),
        "*" => Some(BinaryOperator::Multiplication),
        "/" => Some(BinaryOperator::Division),
        "rem" => Some(BinaryOperator::Remainder),
        "==" => Some(BinaryOperator::StrictEquality),
        "=:=" => Some(BinaryOperator::StrictEquality),
        "!=" | "/=" => Some(BinaryOperator::StrictInequality),
        "=/=" => Some(BinaryOperator::StrictInequality),
        "<" => Some(BinaryOperator::LessThan),
        "<=" => Some(BinaryOperator::LessEqualThan),
        ">" => Some(BinaryOperator::GreaterThan),
        ">=" => Some(BinaryOperator::GreaterEqualThan),
        _ => None,
    }
}
