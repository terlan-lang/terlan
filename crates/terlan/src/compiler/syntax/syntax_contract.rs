use std::{collections::BTreeSet, sync::OnceLock};

use super::ebnf_lexer::{EbnfLexer, EbnfTokenKind};
use crate::terlan_syntax::{
    ebnf::{
        compile_ebnf, EbnfCompileError, EbnfCompileResult, EbnfGrammarContract, EbnfGrammarExpr,
        EbnfGrammarExprKind,
    },
    span::Span,
};

pub const SYNTAX_CONTRACT_ARTIFACT_SCHEMA: &str = "terlan-syntax-contract-v1";
pub const SYNTAX_CONTRACT_FINGERPRINT_ALGORITHM: &str = "fnv1a64";
pub const CANONICAL_TERLAN_EBNF: &str =
    include_str!("../../../../../docs/grammar/TERLAN_SYNTAX_SPEC.ebnf");

/// Compiles the embedded canonical Terlan EBNF.
///
/// Inputs: none; the source is `CANONICAL_TERLAN_EBNF`. Output: grammar
/// contract or EBNF compile error. Transformation: delegates directly to the
/// EBNF compiler with the embedded syntax specification.
pub fn canonical_terlan_syntax_contract() -> EbnfCompileResult<EbnfGrammarContract> {
    compile_ebnf(CANONICAL_TERLAN_EBNF)
}

/// Compiles and validates the embedded canonical Terlan EBNF.
///
/// Inputs: none. Output: checked grammar contract or syntax contract error.
/// Transformation: compiles the embedded EBNF and applies required-rule
/// validation before returning the contract.
pub fn validated_canonical_terlan_syntax_contract(
) -> Result<EbnfGrammarContract, SyntaxContractError> {
    let contract = canonical_terlan_syntax_contract().map_err(SyntaxContractError::Compile)?;
    let diagnostics = validate_syntax_contract(&contract);
    if diagnostics.is_empty() {
        Ok(contract)
    } else {
        Err(SyntaxContractError::Validation(diagnostics))
    }
}

/// Returns the cached validated canonical syntax contract.
///
/// Inputs: none. Output: static contract reference or cached error.
/// Transformation: initializes a `OnceLock` with the validated canonical
/// contract so repeated callers do not recompile EBNF.
pub fn cached_canonical_terlan_syntax_contract(
) -> Result<&'static EbnfGrammarContract, SyntaxContractError> {
    static RESULT: OnceLock<Result<EbnfGrammarContract, SyntaxContractError>> = OnceLock::new();

    match RESULT.get_or_init(validated_canonical_terlan_syntax_contract) {
        Ok(contract) => Ok(contract),
        Err(error) => Err(error.clone()),
    }
}

/// Ensures the canonical syntax contract compiles and validates.
///
/// Inputs: none. Output: `Ok(())` when valid. Transformation: forces cached
/// contract initialization and discards the contract payload.
pub fn ensure_canonical_syntax_contract_valid() -> Result<(), SyntaxContractError> {
    cached_canonical_terlan_syntax_contract().map(|_| ())
}

/// Builds the cached canonical syntax contract artifact.
///
/// Inputs: none. Output: syntax contract artifact or error. Transformation:
/// combines the cached contract with its identity/fingerprint metadata.
pub fn cached_canonical_terlan_syntax_contract_artifact(
) -> Result<SyntaxContractArtifact, SyntaxContractError> {
    let contract = cached_canonical_terlan_syntax_contract()?;
    let identity = syntax_contract_identity(contract)?;
    Ok(SyntaxContractArtifact {
        schema: identity.schema,
        fingerprint_algorithm: identity.fingerprint_algorithm,
        fingerprint: identity.fingerprint,
        contract: contract.clone(),
    })
}

/// Builds the cached canonical syntax contract identity.
///
/// Inputs: none. Output: syntax contract identity or error. Transformation:
/// fingerprints the cached contract and wraps it in artifact identity metadata.
pub fn cached_canonical_terlan_syntax_contract_identity(
) -> Result<SyntaxContractIdentity, SyntaxContractError> {
    let contract = cached_canonical_terlan_syntax_contract()?;
    syntax_contract_identity(contract)
}

/// Serializes the cached canonical syntax contract identity as JSON.
///
/// Inputs: none. Output: JSON string or serialization error. Transformation:
/// builds the cached identity and encodes it with `serde_json`.
pub fn cached_canonical_terlan_syntax_contract_identity_json() -> Result<String, SyntaxContractError>
{
    let identity = cached_canonical_terlan_syntax_contract_identity()?;
    serde_json::to_string(&identity).map_err(|error| {
        SyntaxContractError::Compile(EbnfCompileError::Serialize(error.to_string()))
    })
}

/// Serializes the cached canonical syntax contract artifact as pretty JSON.
///
/// Inputs: none. Output: JSON string or serialization error. Transformation:
/// builds the cached artifact and encodes it with pretty `serde_json` output.
pub fn cached_canonical_terlan_syntax_contract_artifact_json() -> Result<String, SyntaxContractError>
{
    let artifact = cached_canonical_terlan_syntax_contract_artifact()?;
    serde_json::to_string_pretty(&artifact).map_err(|error| {
        SyntaxContractError::Compile(EbnfCompileError::Serialize(error.to_string()))
    })
}

/// Computes a stable fingerprint for a syntax contract.
///
/// Inputs: grammar contract. Output: algorithm-prefixed fingerprint.
/// Transformation: serializes the contract to JSON and hashes the bytes with
/// FNV-1a 64-bit hex.
pub fn syntax_contract_fingerprint(
    contract: &EbnfGrammarContract,
) -> Result<String, SyntaxContractError> {
    let json = serde_json::to_string(contract).map_err(|error| {
        SyntaxContractError::Compile(EbnfCompileError::Serialize(error.to_string()))
    })?;
    Ok(format!(
        "{}:{}",
        SYNTAX_CONTRACT_FINGERPRINT_ALGORITHM,
        fnv1a64_hex(json.as_bytes())
    ))
}

/// Builds an identity object for a syntax contract.
///
/// Inputs: grammar contract. Output: syntax contract identity or error.
/// Transformation: computes the contract fingerprint and wraps it with schema
/// and algorithm metadata.
fn syntax_contract_identity(
    contract: &EbnfGrammarContract,
) -> Result<SyntaxContractIdentity, SyntaxContractError> {
    Ok(syntax_contract_identity_from_fingerprint(
        syntax_contract_fingerprint(contract)?,
    ))
}

/// Builds syntax contract identity metadata from a fingerprint.
///
/// Inputs: fingerprint string. Output: identity object. Transformation: adds
/// current artifact schema and fingerprint algorithm metadata.
pub fn syntax_contract_identity_from_fingerprint(
    fingerprint: impl Into<String>,
) -> SyntaxContractIdentity {
    SyntaxContractIdentity {
        schema: SYNTAX_CONTRACT_ARTIFACT_SCHEMA.to_string(),
        fingerprint_algorithm: SYNTAX_CONTRACT_FINGERPRINT_ALGORITHM.to_string(),
        fingerprint: fingerprint.into(),
    }
}

/// Checks whether an identity matches the current canonical contract.
///
/// Inputs: identity to check. Output: match flag or error. Transformation:
/// compares the supplied identity with the cached current identity.
pub fn syntax_contract_identity_matches_current(
    identity: &SyntaxContractIdentity,
) -> Result<bool, SyntaxContractError> {
    Ok(identity == &cached_canonical_terlan_syntax_contract_identity()?)
}

/// Checks whether an artifact or raw fingerprint matches the current contract.
///
/// Inputs: artifact JSON or raw fingerprint. Output: match flag or error.
/// Transformation: delegates to the detailed artifact check and collapses the
/// result to a boolean.
pub fn syntax_contract_artifact_matches_current(
    artifact_or_fingerprint: &str,
) -> Result<bool, SyntaxContractError> {
    Ok(matches!(
        check_syntax_contract_artifact_against_current(artifact_or_fingerprint)?,
        SyntaxContractArtifactCheck::Match { .. }
    ))
}

/// Compares an artifact or raw fingerprint with the current contract.
///
/// Inputs: artifact JSON or raw fingerprint. Output: match/mismatch/invalid
/// check result. Transformation: extracts the fingerprint and compares it with
/// the cached current artifact fingerprint.
pub fn check_syntax_contract_artifact_against_current(
    artifact_or_fingerprint: &str,
) -> Result<SyntaxContractArtifactCheck, SyntaxContractError> {
    let current = cached_canonical_terlan_syntax_contract_artifact()?.fingerprint;
    let Some(found) = extract_syntax_contract_artifact_fingerprint(artifact_or_fingerprint) else {
        return Ok(SyntaxContractArtifactCheck::InvalidArtifact);
    };
    if found == current {
        Ok(SyntaxContractArtifactCheck::Match { fingerprint: found })
    } else {
        Ok(SyntaxContractArtifactCheck::Mismatch {
            expected: current,
            found,
        })
    }
}

/// Extracts a syntax contract fingerprint from JSON or raw text.
///
/// Inputs: artifact JSON or raw fingerprint text. Output: fingerprint when the
/// schema and algorithm are valid. Transformation: accepts raw `fnv1a64:...`
/// strings or minimal JSON fields without fully deserializing the artifact.
pub fn extract_syntax_contract_artifact_fingerprint(contents: &str) -> Option<String> {
    let trimmed = contents.trim();
    if trimmed.starts_with("fnv1a64:") && !trimmed.contains(char::is_whitespace) {
        return Some(trimmed.to_string());
    }

    let schema = extract_json_string_field(trimmed, "schema")?;
    if schema != SYNTAX_CONTRACT_ARTIFACT_SCHEMA {
        return None;
    }
    let fingerprint_algorithm = extract_json_string_field(trimmed, "fingerprint_algorithm")?;
    if fingerprint_algorithm != SYNTAX_CONTRACT_FINGERPRINT_ALGORITHM {
        return None;
    }

    let fingerprint = extract_json_string_field(trimmed, "fingerprint")?;
    if fingerprint.starts_with("fnv1a64:") && !fingerprint.contains(char::is_whitespace) {
        Some(fingerprint)
    } else {
        None
    }
}

/// Extracts a simple unescaped string field from JSON-like text.
///
/// Inputs: JSON text and field name. Output: unescaped field value or `None`.
/// Transformation: performs a small field scan for artifact preflight without
/// accepting escaped values.
fn extract_json_string_field(contents: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let after_field = contents.get(contents.find(&needle)? + needle.len()..)?;
    let after_colon = after_field.get(after_field.find(':')? + 1..)?.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let end = value.find('"')?;
    let parsed = &value[..end];
    if parsed.contains('\\') {
        None
    } else {
        Some(parsed.to_string())
    }
}

/// Hashes bytes with FNV-1a 64-bit and returns hex text.
///
/// Inputs: byte slice. Output: 16-character lowercase hex hash. Transformation:
/// applies FNV-1a wrapping multiplication over the bytes.
fn fnv1a64_hex(bytes: &[u8]) -> String {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    let mut hash = OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }

    format!("{hash:016x}")
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Serialized syntax contract artifact.
///
/// Inputs: validated grammar contract. Output: schema, fingerprint metadata,
/// and contract payload. Transformation: packages the canonical EBNF contract
/// for generated summaries and CI checks.
pub struct SyntaxContractArtifact {
    pub schema: String,
    pub fingerprint_algorithm: String,
    pub fingerprint: String,
    pub contract: EbnfGrammarContract,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Stable identity for a syntax contract artifact.
///
/// Inputs: contract fingerprint. Output: schema, algorithm, and fingerprint.
/// Transformation: records enough metadata to compare artifacts without
/// embedding the full grammar contract.
pub struct SyntaxContractIdentity {
    pub schema: String,
    pub fingerprint_algorithm: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Result of comparing a syntax contract artifact against the current contract.
///
/// Inputs: parsed fingerprint and current fingerprint. Output: match,
/// mismatch, or invalid-artifact tag. Transformation: preserves mismatch
/// details for diagnostics.
pub enum SyntaxContractArtifactCheck {
    Match { fingerprint: String },
    Mismatch { expected: String, found: String },
    InvalidArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Error returned by syntax contract compilation or validation.
///
/// Inputs: EBNF compile error or validation diagnostics. Output: typed error.
/// Transformation: keeps compile and validation failures distinct for callers.
pub enum SyntaxContractError {
    Compile(EbnfCompileError),
    Validation(Vec<SyntaxContractDiagnostic>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Syntax contract validation diagnostic.
///
/// Inputs: invalid contract condition. Output: source span and message.
/// Transformation: maps grammar contract validation failures to parser spans.
pub struct SyntaxContractDiagnostic {
    pub span: Span,
    pub message: String,
}

/// Machine-readable finding emitted by generic EBNF validation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EbnfValidationFinding {
    pub line: usize,
    pub kind: String,
    pub message: String,
}

/// Generic EBNF validation result consumed by typed repository tooling.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EbnfValidationReport {
    pub schema: String,
    pub definitions: usize,
    pub undefined_symbols: Vec<EbnfValidationFinding>,
    pub unreachable_symbols: Vec<EbnfValidationFinding>,
    pub strict_findings: Vec<EbnfValidationFinding>,
}

impl EbnfValidationReport {
    /// Returns whether every enabled EBNF validation class passed.
    pub fn is_valid(&self) -> bool {
        self.undefined_symbols.is_empty()
            && self.unreachable_symbols.is_empty()
            && self.strict_findings.is_empty()
    }
}

/// Compiles and generically validates one EBNF source document.
pub fn validate_ebnf_source(source: &str, strict: bool) -> EbnfCompileResult<EbnfValidationReport> {
    let contract = compile_ebnf(source)?;
    let names = contract
        .rules
        .iter()
        .map(|rule| rule.name.clone())
        .collect::<BTreeSet<_>>();
    let mut undefined_symbols = Vec::new();
    let mut references = std::collections::BTreeMap::<String, BTreeSet<String>>::new();
    for rule in &contract.rules {
        let mut rule_references = BTreeSet::new();
        collect_generic_references(
            source,
            &rule.expr,
            &names,
            &mut rule_references,
            &mut undefined_symbols,
        );
        references.insert(rule.name.clone(), rule_references);
    }
    undefined_symbols
        .sort_by(|left, right| (left.line, &left.message).cmp(&(right.line, &right.message)));
    undefined_symbols.dedup();

    let mut reachable = BTreeSet::new();
    let mut pending = contract.entry_rule.iter().cloned().collect::<Vec<_>>();
    while let Some(name) = pending.pop() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        if let Some(referenced) = references.get(&name) {
            pending.extend(referenced.iter().cloned());
        }
    }
    let entry = contract.entry_rule.as_deref().unwrap_or("<missing>");
    let mut unreachable_symbols = contract
        .rules
        .iter()
        .filter(|rule| !reachable.contains(&rule.name))
        .map(|rule| EbnfValidationFinding {
            line: source_line(source, rule.name_span.start),
            kind: "unreachable-symbol".to_string(),
            message: format!("production '{}' is unreachable from '{}'", rule.name, entry),
        })
        .collect::<Vec<_>>();
    unreachable_symbols
        .sort_by(|left, right| (&left.message, left.line).cmp(&(&right.message, right.line)));

    let mut strict_findings = Vec::new();
    if strict {
        for token in EbnfLexer::new(source)
            .lex()
            .map_err(|error| EbnfCompileError::Parse(error.message, error.span))?
        {
            let message = match token.kind {
                EbnfTokenKind::CharacterClass(_) => {
                    Some("regex-style character class is not strict EBNF")
                }
                EbnfTokenKind::Star => Some("'*' repetition shorthand is not strict EBNF"),
                EbnfTokenKind::Plus => Some("'+' repetition shorthand is not strict EBNF"),
                _ => None,
            };
            if let Some(message) = message {
                strict_findings.push(EbnfValidationFinding {
                    line: source_line(source, token.span.start),
                    kind: "strict-ebnf".to_string(),
                    message: message.to_string(),
                });
            }
        }
    }

    Ok(EbnfValidationReport {
        schema: "terlan.ebnf-validation.v1".to_string(),
        definitions: contract.rules.len(),
        undefined_symbols,
        unreachable_symbols,
        strict_findings,
    })
}

fn collect_generic_references(
    source: &str,
    expr: &EbnfGrammarExpr,
    names: &BTreeSet<String>,
    references: &mut BTreeSet<String>,
    findings: &mut Vec<EbnfValidationFinding>,
) {
    match &expr.kind {
        EbnfGrammarExprKind::Nonterminal { name } => {
            if names.contains(name) {
                references.insert(name.clone());
            } else {
                findings.push(EbnfValidationFinding {
                    line: source_line(source, expr.span.start),
                    kind: "undefined-symbol".to_string(),
                    message: format!("reference to '{name}' is not defined"),
                });
            }
        }
        EbnfGrammarExprKind::Sequence { items } | EbnfGrammarExprKind::Alternation { items } => {
            for item in items {
                collect_generic_references(source, item, names, references, findings);
            }
        }
        EbnfGrammarExprKind::Optional { expr }
        | EbnfGrammarExprKind::Repetition { expr }
        | EbnfGrammarExprKind::Group { expr }
        | EbnfGrammarExprKind::OneOrMore { expr } => {
            collect_generic_references(source, expr, names, references, findings);
        }
        EbnfGrammarExprKind::Terminal { .. }
        | EbnfGrammarExprKind::CharacterClass { .. }
        | EbnfGrammarExprKind::Special { .. } => {}
    }
}

fn source_line(source: &str, byte: usize) -> usize {
    source
        .as_bytes()
        .iter()
        .take(byte.min(source.len()))
        .filter(|value| **value == b'\n')
        .count()
        + 1
}

/// Validates the required shape of the Terlan syntax contract.
///
/// Inputs: compiled grammar contract. Output: validation diagnostics.
/// Transformation: checks entry rule, required rules, and required rule
/// references that the formal compiler path depends on.
pub fn validate_syntax_contract(contract: &EbnfGrammarContract) -> Vec<SyntaxContractDiagnostic> {
    let mut diagnostics = Vec::new();

    if contract.entry_rule.as_deref() != Some("SyntaxSpec") {
        diagnostics.push(SyntaxContractDiagnostic {
            span: Span::new(0, 0),
            message: "syntax contract entry rule must be SyntaxSpec".to_string(),
        });
    }

    require_all_rules_reachable(contract, &mut diagnostics);

    for rule in REQUIRED_SYNTAX_RULES {
        if contract.rule(rule).is_none() {
            diagnostics.push(SyntaxContractDiagnostic {
                span: Span::new(0, 0),
                message: format!("syntax contract is missing required rule {rule}"),
            });
        }
    }

    require_rule_reference(contract, "SyntaxSpec", "Program", &mut diagnostics);
    require_rule_reference(contract, "Program", "Declaration", &mut diagnostics);
    require_rule_reference(contract, "Declaration", "Annotation", &mut diagnostics);
    require_rule_reference(contract, "Declaration", "DeclarationCore", &mut diagnostics);
    require_rule_reference(
        contract,
        "DeclarationCore",
        "AnnotationSchemaDecl",
        &mut diagnostics,
    );
    require_rule_reference(
        contract,
        "AnnotationBlock",
        "AnnotationItem",
        &mut diagnostics,
    );
    require_rule_reference(
        contract,
        "AnnotationItem",
        "AnnotationEntry",
        &mut diagnostics,
    );
    require_rule_reference(
        contract,
        "AnnotationItem",
        "AnnotationValue",
        &mut diagnostics,
    );
    require_rule_reference(
        contract,
        "AnnotationEntry",
        "AnnotationValue",
        &mut diagnostics,
    );
    require_rule_reference(
        contract,
        "AnnotationValue",
        "AnnotationQualifiedName",
        &mut diagnostics,
    );
    require_rule_reference(
        contract,
        "AnnotationValue",
        "AnnotationList",
        &mut diagnostics,
    );
    require_rule_reference(
        contract,
        "AnnotationValue",
        "AnnotationObject",
        &mut diagnostics,
    );
    require_rule_reference(
        contract,
        "AnnotationSchemaDecl",
        "AnnotationSchemaEntry",
        &mut diagnostics,
    );
    require_rule_reference(
        contract,
        "AnnotationSchemaEntry",
        "AnnotationKeySchema",
        &mut diagnostics,
    );
    require_rule_reference(
        contract,
        "AnnotationKeySchema",
        "AnnotationValueType",
        &mut diagnostics,
    );
    require_rule_reference(
        contract,
        "AnnotationKeySchema",
        "AnnotationKeyOptions",
        &mut diagnostics,
    );
    require_rule_reference(
        contract,
        "AnnotationKeyOption",
        "AnnotationTargetSet",
        &mut diagnostics,
    );
    require_rule_reference(contract, "DeclarationCore", "ConfigDecl", &mut diagnostics);
    require_rule_reference(
        contract,
        "DeclarationCore",
        "TraitImplDecl",
        &mut diagnostics,
    );
    require_rule_reference(contract, "ConfigDecl", "MetadataBlock", &mut diagnostics);
    require_rule_reference(contract, "Expr", "LetExpr", &mut diagnostics);
    require_rule_reference(contract, "Expr", "AssignExpr", &mut diagnostics);
    require_rule_reference(contract, "AssignExpr", "IndexAssignExpr", &mut diagnostics);
    require_rule_reference(contract, "AssignExpr", "PipeExpr", &mut diagnostics);
    require_rule_reference(contract, "IndexAssignExpr", "PostfixExpr", &mut diagnostics);
    require_rule_reference(contract, "IndexAssignExpr", "Expr", &mut diagnostics);
    require_rule_reference(contract, "LetExpr", "LetBinding", &mut diagnostics);
    require_rule_reference(contract, "LetExpr", "Expr", &mut diagnostics);
    require_rule_reference(contract, "LetBinding", "Pattern", &mut diagnostics);
    require_rule_reference(contract, "LetBinding", "Expr", &mut diagnostics);
    require_rule_reference(contract, "PipeExpr", "OrExpr", &mut diagnostics);
    require_rule_reference(contract, "OrExpr", "AndExpr", &mut diagnostics);
    require_rule_reference(contract, "OrExpr", "OrOp", &mut diagnostics);
    require_rule_reference(contract, "AndExpr", "CmpExpr", &mut diagnostics);
    require_rule_reference(contract, "AndExpr", "AndOp", &mut diagnostics);
    require_rule_reference(contract, "CmpExpr", "AddExpr", &mut diagnostics);
    require_rule_reference(contract, "AddExpr", "MulExpr", &mut diagnostics);
    require_rule_reference(contract, "MulExpr", "CastExpr", &mut diagnostics);
    require_rule_reference(contract, "CastExpr", "UnaryExpr", &mut diagnostics);
    require_rule_reference(contract, "CastExpr", "TypeExpr", &mut diagnostics);
    require_rule_reference(contract, "UnaryExpr", "PostfixExpr", &mut diagnostics);
    require_rule_reference(contract, "PostfixExpr", "PrimaryExpr", &mut diagnostics);
    require_rule_reference(contract, "PrimaryExpr", "CaseExpr", &mut diagnostics);
    require_rule_reference(contract, "PrimaryExpr", "LambdaExpr", &mut diagnostics);
    require_rule_reference(contract, "PrimaryExpr", "TryExpr", &mut diagnostics);
    require_rule_reference(contract, "PrimaryExpr", "IfExpr", &mut diagnostics);
    require_rule_reference(contract, "CallExpr", "NameRef", &mut diagnostics);
    require_rule_reference(contract, "TypeRef", "ModulePath", &mut diagnostics);
    require_rule_reference(contract, "TypeRef", "TypeName", &mut diagnostics);
    require_rule_reference(contract, "Pattern", "PrimaryPattern", &mut diagnostics);
    require_rule_reference(contract, "ListPattern", "Pattern", &mut diagnostics);

    diagnostics
}

const REQUIRED_SYNTAX_RULES: &[&str] = &[
    "SyntaxSpec",
    "Program",
    "Declaration",
    "DeclarationCore",
    "Annotation",
    "AnnotationBlock",
    "AnnotationItem",
    "AnnotationEntry",
    "AnnotationValue",
    "AnnotationQualifiedName",
    "AnnotationList",
    "AnnotationObject",
    "AnnotationSchemaDecl",
    "AnnotationKeySchema",
    "AnnotationKeyOptions",
    "AnnotationTargetSet",
    "AnnotationValueType",
    "ModuleDecl",
    "ImportDecl",
    "TypeDecl",
    "OpaqueTypeDecl",
    "StructDecl",
    "ConstructorDecl",
    "TraitDecl",
    "TraitImplDecl",
    "FunctionDecl",
    "ConfigDecl",
    "MetadataBlock",
    "TypeExpr",
    "Expr",
    "LetExpr",
    "LetBinding",
    "OrExpr",
    "OrOp",
    "AndExpr",
    "AndOp",
    "CastExpr",
    "PrimaryExpr",
    "Pattern",
    "CallExpr",
];

/// Requires one grammar rule to reference another rule.
///
/// Inputs: contract, rule name, referenced rule, and diagnostic sink. Output:
/// diagnostics may be appended. Transformation: finds the rule and recursively
/// checks its expression tree for the referenced nonterminal.
fn require_rule_reference(
    contract: &EbnfGrammarContract,
    rule_name: &str,
    referenced_rule: &str,
    diagnostics: &mut Vec<SyntaxContractDiagnostic>,
) {
    let Some(rule) = contract.rule(rule_name) else {
        return;
    };

    if !expr_references_rule(&rule.expr, referenced_rule) {
        diagnostics.push(SyntaxContractDiagnostic {
            span: rule.name_span.into(),
            message: format!("syntax rule {rule_name} must reference {referenced_rule}"),
        });
    }
}

/// Requires every production to be reachable from the declared entry rule.
fn require_all_rules_reachable(
    contract: &EbnfGrammarContract,
    diagnostics: &mut Vec<SyntaxContractDiagnostic>,
) {
    let Some(entry_rule) = contract.entry_rule.as_deref() else {
        return;
    };
    let mut reachable = BTreeSet::new();
    let mut pending = vec![entry_rule.to_string()];
    while let Some(name) = pending.pop() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        let Some(rule) = contract.rule(&name) else {
            continue;
        };
        collect_rule_references(&rule.expr, &mut pending);
    }
    for rule in &contract.rules {
        if !reachable.contains(&rule.name) {
            diagnostics.push(SyntaxContractDiagnostic {
                span: rule.name_span.into(),
                message: format!("syntax rule {} is unreachable from {entry_rule}", rule.name),
            });
        }
    }
}

/// Collects direct nonterminal references from one EBNF expression tree.
fn collect_rule_references(expr: &EbnfGrammarExpr, references: &mut Vec<String>) {
    match &expr.kind {
        EbnfGrammarExprKind::Nonterminal { name } => references.push(name.clone()),
        EbnfGrammarExprKind::Sequence { items } | EbnfGrammarExprKind::Alternation { items } => {
            for item in items {
                collect_rule_references(item, references);
            }
        }
        EbnfGrammarExprKind::Optional { expr }
        | EbnfGrammarExprKind::Repetition { expr }
        | EbnfGrammarExprKind::Group { expr }
        | EbnfGrammarExprKind::OneOrMore { expr } => collect_rule_references(expr, references),
        EbnfGrammarExprKind::Terminal { .. }
        | EbnfGrammarExprKind::CharacterClass { .. }
        | EbnfGrammarExprKind::Special { .. } => {}
    }
}

/// Returns whether an EBNF expression references a rule.
///
/// Inputs: grammar expression and rule name. Output: reference flag.
/// Transformation: recursively scans nested expression forms for a matching
/// nonterminal.
fn expr_references_rule(expr: &EbnfGrammarExpr, rule_name: &str) -> bool {
    match &expr.kind {
        EbnfGrammarExprKind::Nonterminal { name } => name == rule_name,
        EbnfGrammarExprKind::Sequence { items } | EbnfGrammarExprKind::Alternation { items } => {
            items
                .iter()
                .any(|item| expr_references_rule(item, rule_name))
        }
        EbnfGrammarExprKind::Optional { expr }
        | EbnfGrammarExprKind::Repetition { expr }
        | EbnfGrammarExprKind::Group { expr }
        | EbnfGrammarExprKind::OneOrMore { expr } => expr_references_rule(expr, rule_name),
        EbnfGrammarExprKind::Terminal { .. }
        | EbnfGrammarExprKind::CharacterClass { .. }
        | EbnfGrammarExprKind::Special { .. } => false,
    }
}

#[cfg(test)]
#[path = "syntax_contract_test.rs"]
#[cfg(test)]
mod syntax_contract_test;
