use super::expression_formatting::format_expr;
use super::{BinaryOp, Expr};

const PRECEDENCE_ASSIGNMENT: u8 = 0;
const PRECEDENCE_CONSTRUCTOR_CHAIN: u8 = 1;
const PRECEDENCE_PIPE: u8 = 2;
const PRECEDENCE_OR: u8 = 3;
const PRECEDENCE_AND: u8 = 4;
const PRECEDENCE_COMPARE: u8 = 5;
const PRECEDENCE_ADD: u8 = 6;
const PRECEDENCE_MULTIPLY: u8 = 7;
const PRECEDENCE_CAST: u8 = 8;
const PRECEDENCE_UNARY: u8 = 9;
const PRECEDENCE_POSTFIX: u8 = 10;
const PRECEDENCE_PRIMARY: u8 = 11;

fn binary_precedence(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::PipeForward => PRECEDENCE_PIPE,
        BinaryOp::Or => PRECEDENCE_OR,
        BinaryOp::And => PRECEDENCE_AND,
        BinaryOp::EqEq
        | BinaryOp::NotEq
        | BinaryOp::Lt
        | BinaryOp::Gt
        | BinaryOp::LtEq
        | BinaryOp::GtEq
        | BinaryOp::In => PRECEDENCE_COMPARE,
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Range => PRECEDENCE_ADD,
        BinaryOp::Mul | BinaryOp::Div | BinaryOp::DivRem | BinaryOp::Rem => PRECEDENCE_MULTIPLY,
    }
}

fn expression_precedence(expr: &Expr) -> u8 {
    match expr {
        Expr::Let { .. }
        | Expr::IndexAssign { .. }
        | Expr::Fun { .. }
        | Expr::Quote(_)
        | Expr::Sequence(_) => PRECEDENCE_ASSIGNMENT,
        Expr::ConstructorChain { .. } => PRECEDENCE_CONSTRUCTOR_CHAIN,
        Expr::BinaryOp { op, .. } => binary_precedence(*op),
        Expr::Cast { .. } => PRECEDENCE_CAST,
        Expr::UnaryOp { .. } => PRECEDENCE_UNARY,
        Expr::Call { .. }
        | Expr::Index(_, _)
        | Expr::RecordAccess { .. }
        | Expr::FieldAccess { .. }
        | Expr::RecordUpdate { .. } => PRECEDENCE_POSTFIX,
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Atom(_)
        | Expr::AtomLiteral(_)
        | Expr::Binary(_)
        | Expr::Var(_)
        | Expr::Tuple(_)
        | Expr::List(_)
        | Expr::ListCons(_, _)
        | Expr::FixedArray(_)
        | Expr::Map(_)
        | Expr::ListComprehension { .. }
        | Expr::Case { .. }
        | Expr::Try { .. }
        | Expr::If { .. }
        | Expr::MacroCall { .. }
        | Expr::RawMacro { .. }
        | Expr::HtmlBlock(_)
        | Expr::RecordConstruct { .. }
        | Expr::BinaryLayout { .. }
        | Expr::Unquote(_) => PRECEDENCE_PRIMARY,
    }
}

pub(super) fn parenthesize_if_needed(rendered: String, needed: bool) -> String {
    if needed {
        format!("({rendered})")
    } else {
        rendered
    }
}

pub(super) fn format_binary_operand(expr: &Expr, parent: BinaryOp, is_right: bool) -> String {
    let child_precedence = expression_precedence(expr);
    let parent_precedence = binary_precedence(parent);
    parenthesize_if_needed(
        format_expr(expr, 0),
        child_precedence < parent_precedence || (is_right && child_precedence == parent_precedence),
    )
}

pub(super) fn format_unary_operand(expr: &Expr) -> String {
    parenthesize_if_needed(
        format_expr(expr, 0),
        expression_precedence(expr) < PRECEDENCE_UNARY,
    )
}

pub(super) fn format_cast_operand(expr: &Expr) -> String {
    parenthesize_if_needed(
        format_expr(expr, 0),
        expression_precedence(expr) < PRECEDENCE_CAST,
    )
}

pub(super) fn format_postfix_base(expr: &Expr, indent: usize) -> String {
    parenthesize_if_needed(
        format_expr(expr, indent),
        expression_precedence(expr) < PRECEDENCE_POSTFIX,
    )
}

pub(super) fn format_constructor_chain_operand(expr: &Expr, is_right: bool) -> String {
    let precedence = expression_precedence(expr);
    parenthesize_if_needed(
        format_expr(expr, 0),
        precedence < PRECEDENCE_CONSTRUCTOR_CHAIN
            || (is_right && precedence == PRECEDENCE_CONSTRUCTOR_CHAIN),
    )
}
