//! Finite decimal formatting shared by explicit conversion and interpolation.

use crate::terlan_typeck::CoreExpr;

const FORMAT: &str = r#"((value) => {
    if (!Number.isFinite(value)) throw new RangeError("Float must be finite");
    if (Object.is(value, -0)) return "-0";
    const text = String(value);
    const split = text.split("e");
    if (split.length === 1) return text;
    const negative = split[0][0] === "-";
    const mantissa = negative ? split[0].slice(1) : split[0];
    const decimal = mantissa.indexOf(".");
    const digits = mantissa.replace(".", "");
    const point = (decimal < 0 ? mantissa.length : decimal) + Number(split[1]);
    const result = point <= 0 ? "0." + "0".repeat(-point) + digits
        : point >= digits.length ? digits + "0".repeat(point - digits.length)
        : digits.slice(0, point) + "." + digits.slice(point);
    return (negative ? "-" : "") + result;
})"#;

pub(super) fn emit<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    let [value] = args else { return None };
    let callee = oxc_parser::Parser::new(ast.allocator, FORMAT, oxc_span::SourceType::mjs())
        .parse_expression()
        .expect("compiler-owned float formatter");
    Some(ast.expression_call(
        oxc_span::SPAN,
        callee,
        oxc_ast::NONE,
        ast.vec1(oxc_ast::ast::Argument::from(
            super::direct_ast::core_expr_to_oxc_expression(ast, value)?,
        )),
        false,
    ))
}

#[cfg(test)]
#[path = "float_format_test.rs"]
mod tests;
