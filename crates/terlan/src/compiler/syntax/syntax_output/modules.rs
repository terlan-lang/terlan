use serde::{Deserialize, Serialize};

use crate::terlan_syntax::{
    ebnf::{EbnfGrammarContract, EbnfSourceSpan},
    syntax_contract::SyntaxContractIdentity,
};

use super::SyntaxDeclarationOutput;

pub const SYNTAX_MODULE_OUTPUT_SCHEMA: &str = "terlan-syntax-module-output-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Source kind represented by a syntax-output module.
///
/// Inputs:
/// - Parser entrypoint used to parse the source file.
///
/// Outputs:
/// - Stable serialized source-kind tag.
///
/// Transformation:
/// - Distinguishes implementation modules from interface modules for later
///   phases without relying on file extensions.
pub enum SyntaxSourceKind {
    Module,
    Interface,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Serializable syntax-output module.
///
/// Inputs:
/// - Parsed module, syntax contract identity, grammar contract, and docs.
///
/// Outputs:
/// - Complete source-level module payload consumed by HIR, typecheck, and
///   formal contract tests.
///
/// Transformation:
/// - Converts parser-owned module state into a deterministic compiler artifact.
pub struct SyntaxModuleOutput {
    pub schema: String,
    pub source_kind: SyntaxSourceKind,
    pub syntax_contract: SyntaxContractIdentity,
    pub module_name: String,
    pub docs: Vec<String>,
    pub span: EbnfSourceSpan,
    pub declarations: Vec<SyntaxDeclarationOutput>,
    pub contract: EbnfGrammarContract,
}
