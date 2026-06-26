//! Erlang operator render identities.
//!
//! This module maps lowered operator identities to Erlang operator tokens.

/// Erlang binary operator identity.
///
/// Inputs:
/// - Terlan/CoreIR operator lowering decisions.
///
/// Output:
/// - Backend operator spelling through `render`.
///
/// Transformation:
/// - Keeps source/operator normalization separate from expression rendering.
#[derive(Debug, Clone)]
pub(in crate::emit) enum ErlBinaryOp {
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
    Rem,
    And,
    Or,
    PipeForward,
    Send,
}

/// Erlang unary operator identity.
///
/// Inputs:
/// - Terlan/CoreIR unary operator lowering decisions.
///
/// Output:
/// - Backend unary operator spelling selected during expression rendering.
///
/// Transformation:
/// - Represents negation and logical-not independently from operand lowering.
#[derive(Debug, Clone)]
pub(in crate::emit) enum ErlUnaryOp {
    Neg,
    Not,
}

impl ErlBinaryOp {
    /// Renders an Erlang binary operator token.
    ///
    /// Input is a lowered operator identity. Output is the Erlang token used by
    /// expression rendering. The transformation maps Terlan/CoreIR logical and
    /// arithmetic identities onto BEAM-compatible operator spellings.
    pub(in crate::emit) fn render(&self) -> &'static str {
        match self {
            ErlBinaryOp::Add => "+",
            ErlBinaryOp::Sub => "-",
            ErlBinaryOp::Mul => "*",
            ErlBinaryOp::Div => "/",
            ErlBinaryOp::Eq => "==",
            ErlBinaryOp::EqEq => "=:=",
            ErlBinaryOp::EqEqEq => "=:=",
            ErlBinaryOp::NotEq => "/=",
            ErlBinaryOp::NotEqEq => "=/=",
            ErlBinaryOp::GtEq => ">=",
            ErlBinaryOp::Lt => "<",
            ErlBinaryOp::Gt => ">",
            ErlBinaryOp::LtEq => "=<",
            ErlBinaryOp::DivRem => "div",
            ErlBinaryOp::Rem => "rem",
            ErlBinaryOp::And => "andalso",
            ErlBinaryOp::Or => "orelse",
            ErlBinaryOp::PipeForward => "|>",
            ErlBinaryOp::Send => "!",
        }
    }
}
