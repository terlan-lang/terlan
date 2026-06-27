use serde::{Deserialize, Serialize};

use crate::terlan_syntax::ebnf::EbnfSourceSpan;

use super::{SyntaxHtmlNodeOutput, SyntaxPatternOutput, SyntaxTypeOutput};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Serializable expression node emitted by the formal syntax-output pipeline.
///
/// Inputs:
/// - Parsed expression data plus source-span metadata.
///
/// Outputs:
/// - Backend-neutral expression payload for HIR, typecheck, and contract tests.
///
/// Transformation:
/// - Preserves expression kind, textual payloads, children, clauses, fields,
///   patterns, and HTML children without exposing parser-internal enum shapes.
pub struct SyntaxExprOutput {
    pub kind: SyntaxExprKind,
    pub arity: usize,
    pub text: Option<String>,
    #[serde(default)]
    pub span: EbnfSourceSpan,
    #[serde(default)]
    pub raw: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_args: Vec<SyntaxTypeOutput>,
    pub operator: Option<String>,
    pub remote: Option<String>,
    #[serde(default, skip_serializing_if = "arg_names_are_empty")]
    pub arg_names: Vec<Option<String>>,
    pub children: Vec<SyntaxExprOutput>,
    pub patterns: Vec<SyntaxPatternOutput>,
    pub fields: Vec<SyntaxExprFieldOutput>,
    pub clauses: Vec<SyntaxClauseOutput>,
    #[serde(default)]
    pub catch_clauses: Vec<SyntaxClauseOutput>,
    #[serde(default)]
    pub try_after: Option<SyntaxTryAfterOutput>,
    #[serde(default)]
    pub html_nodes: Vec<SyntaxHtmlNodeOutput>,
}

/// Returns whether a call argument-name vector carries no source names.
///
/// Inputs:
/// - Optional call-site names parallel to an expression argument vector.
///
/// Output:
/// - `true` when the vector is empty or contains only positional markers.
///
/// Transformation:
/// - Used by serde to omit empty/default call metadata from syntax output.
fn arg_names_are_empty(arg_names: &Vec<Option<String>>) -> bool {
    arg_names.iter().all(Option::is_none)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Keyed expression field emitted for map, record, or struct-like expressions.
///
/// Inputs:
/// - Parsed field key, requiredness, and value expression.
///
/// Outputs:
/// - Serializable field payload nested under `SyntaxExprOutput`.
///
/// Transformation:
/// - Boxes the value expression so recursive syntax output has stable ownership.
pub struct SyntaxExprFieldOutput {
    pub key: String,
    pub required: bool,
    pub value: Box<SyntaxExprOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Clause payload for branch-based expression forms.
///
/// Inputs:
/// - Parsed clause patterns, optional guard, and body expression.
///
/// Outputs:
/// - Serializable clause representation used by case, if, try, and receive-like
///   expression outputs.
///
/// Transformation:
/// - Normalizes branch components into one payload shape for downstream phases.
pub struct SyntaxClauseOutput {
    pub patterns: Vec<SyntaxPatternOutput>,
    pub guard: Option<Box<SyntaxExprOutput>>,
    pub body: Box<SyntaxExprOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Cleanup payload for try-after syntax output.
///
/// Inputs:
/// - Parsed after-trigger expression and cleanup body.
///
/// Outputs:
/// - Serializable try-after node attached to a try expression.
///
/// Transformation:
/// - Keeps cleanup flow explicit so later phases do not infer it from raw text.
pub struct SyntaxTryAfterOutput {
    pub trigger: Box<SyntaxExprOutput>,
    pub body: Box<SyntaxExprOutput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Expression kind tag used by serialized syntax output.
///
/// Inputs:
/// - Parser expression variants.
///
/// Outputs:
/// - Stable snake-case JSON tags for syntax contracts and downstream compiler
///   phases.
///
/// Transformation:
/// - Decouples serialized expression identity from parser enum names.
pub enum SyntaxExprKind {
    Int,
    Float,
    Atom,
    Binary,
    Var,
    Tuple,
    List,
    ListCons,
    FixedArray,
    Index,
    IndexAssign,
    Map,
    ListComprehension,
    Let,
    Call,
    Case,
    Try,
    If,
    Fun,
    FunctionCall,
    RemoteFunRef,
    Macro,
    RawMacro,
    HtmlBlock,
    RecordAccess,
    FieldAccess,
    RecordUpdate,
    RecordConstruct,
    TemplateInstantiate,
    ConstructorChain,
    UnaryOp,
    Cast,
    BinaryOp,
    Quote,
    Unquote,
    Sequence,
}
