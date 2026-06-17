use std::sync::OnceLock;

use crate::{
    ebnf::{
        compile_ebnf, EbnfCompileError, EbnfCompileResult, EbnfGrammarContract, EbnfGrammarExpr,
        EbnfGrammarExprKind,
    },
    span::Span,
};

pub const SYNTAX_CONTRACT_ARTIFACT_SCHEMA: &str = "terlan-syntax-contract-v1";
pub const SYNTAX_CONTRACT_FINGERPRINT_ALGORITHM: &str = "fnv1a64";
pub const CANONICAL_TERLAN_EBNF: &str =
    include_str!("../../../docs/grammar/TERLAN_SYNTAX_SPEC.ebnf");

pub fn canonical_terlan_syntax_contract() -> EbnfCompileResult<EbnfGrammarContract> {
    compile_ebnf(CANONICAL_TERLAN_EBNF)
}

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

pub fn cached_canonical_terlan_syntax_contract(
) -> Result<&'static EbnfGrammarContract, SyntaxContractError> {
    static RESULT: OnceLock<Result<EbnfGrammarContract, SyntaxContractError>> = OnceLock::new();

    match RESULT.get_or_init(validated_canonical_terlan_syntax_contract) {
        Ok(contract) => Ok(contract),
        Err(error) => Err(error.clone()),
    }
}

pub fn ensure_canonical_syntax_contract_valid() -> Result<(), SyntaxContractError> {
    cached_canonical_terlan_syntax_contract().map(|_| ())
}

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

pub fn cached_canonical_terlan_syntax_contract_identity(
) -> Result<SyntaxContractIdentity, SyntaxContractError> {
    let contract = cached_canonical_terlan_syntax_contract()?;
    syntax_contract_identity(contract)
}

pub fn cached_canonical_terlan_syntax_contract_identity_json() -> Result<String, SyntaxContractError>
{
    let identity = cached_canonical_terlan_syntax_contract_identity()?;
    serde_json::to_string(&identity).map_err(|error| {
        SyntaxContractError::Compile(EbnfCompileError::Serialize(error.to_string()))
    })
}

pub fn cached_canonical_terlan_syntax_contract_artifact_json() -> Result<String, SyntaxContractError>
{
    let artifact = cached_canonical_terlan_syntax_contract_artifact()?;
    serde_json::to_string_pretty(&artifact).map_err(|error| {
        SyntaxContractError::Compile(EbnfCompileError::Serialize(error.to_string()))
    })
}

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

fn syntax_contract_identity(
    contract: &EbnfGrammarContract,
) -> Result<SyntaxContractIdentity, SyntaxContractError> {
    Ok(syntax_contract_identity_from_fingerprint(
        syntax_contract_fingerprint(contract)?,
    ))
}

pub fn syntax_contract_identity_from_fingerprint(
    fingerprint: impl Into<String>,
) -> SyntaxContractIdentity {
    SyntaxContractIdentity {
        schema: SYNTAX_CONTRACT_ARTIFACT_SCHEMA.to_string(),
        fingerprint_algorithm: SYNTAX_CONTRACT_FINGERPRINT_ALGORITHM.to_string(),
        fingerprint: fingerprint.into(),
    }
}

pub fn syntax_contract_identity_matches_current(
    identity: &SyntaxContractIdentity,
) -> Result<bool, SyntaxContractError> {
    Ok(identity == &cached_canonical_terlan_syntax_contract_identity()?)
}

pub fn syntax_contract_artifact_matches_current(
    artifact_or_fingerprint: &str,
) -> Result<bool, SyntaxContractError> {
    Ok(matches!(
        check_syntax_contract_artifact_against_current(artifact_or_fingerprint)?,
        SyntaxContractArtifactCheck::Match { .. }
    ))
}

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
pub struct SyntaxContractArtifact {
    pub schema: String,
    pub fingerprint_algorithm: String,
    pub fingerprint: String,
    pub contract: EbnfGrammarContract,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyntaxContractIdentity {
    pub schema: String,
    pub fingerprint_algorithm: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxContractArtifactCheck {
    Match { fingerprint: String },
    Mismatch { expected: String, found: String },
    InvalidArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxContractError {
    Compile(EbnfCompileError),
    Validation(Vec<SyntaxContractDiagnostic>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxContractDiagnostic {
    pub span: Span,
    pub message: String,
}

pub fn validate_syntax_contract(contract: &EbnfGrammarContract) -> Vec<SyntaxContractDiagnostic> {
    let mut diagnostics = Vec::new();

    if contract.entry_rule.as_deref() != Some("SyntaxSpec") {
        diagnostics.push(SyntaxContractDiagnostic {
            span: Span::new(0, 0),
            message: "syntax contract entry rule must be SyntaxSpec".to_string(),
        });
    }

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
    require_rule_reference(contract, "LetBinding", "Binding", &mut diagnostics);
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
mod tests {
    use super::*;

    #[test]
    fn canonical_contract_compiles_from_embedded_ebnf() {
        let contract =
            canonical_terlan_syntax_contract().expect("compile canonical syntax contract");

        assert_eq!(contract.format_version, 1);
        assert_eq!(contract.entry_rule.as_deref(), Some("SyntaxSpec"));
        assert!(contract.rule("Declaration").is_some());
        assert!(contract.rule("Expr").is_some());
        assert!(matches!(
            contract.rule("PrimaryExpr").expect("PrimaryExpr").expr.kind,
            EbnfGrammarExprKind::Alternation { .. }
        ));
    }

    #[test]
    fn canonical_contract_matches_direct_ebnf_compile() {
        let embedded =
            canonical_terlan_syntax_contract().expect("compile embedded syntax contract");
        let direct = compile_ebnf(CANONICAL_TERLAN_EBNF).expect("compile direct syntax contract");

        assert_eq!(embedded, direct);
    }

    #[test]
    fn validator_accepts_canonical_contract() {
        let contract =
            canonical_terlan_syntax_contract().expect("compile canonical syntax contract");

        let diagnostics = validate_syntax_contract(&contract);
        assert!(
            diagnostics.is_empty(),
            "unexpected syntax contract diagnostics: {diagnostics:?}"
        );
    }

    #[test]
    fn validated_canonical_contract_returns_checked_contract() {
        let contract = validated_canonical_terlan_syntax_contract()
            .expect("validated canonical syntax contract");

        assert_eq!(contract.entry_rule.as_deref(), Some("SyntaxSpec"));
    }

    #[test]
    fn cached_canonical_contract_validation_accepts_canonical_contract() {
        ensure_canonical_syntax_contract_valid().expect("cached syntax validation");
        ensure_canonical_syntax_contract_valid().expect("cached syntax validation is reusable");
    }

    #[test]
    fn cached_canonical_contract_returns_stable_contract_reference() {
        let first = cached_canonical_terlan_syntax_contract().expect("cached syntax contract");
        let second = cached_canonical_terlan_syntax_contract().expect("cached syntax contract");

        assert!(std::ptr::eq(first, second));
        assert_eq!(first.entry_rule.as_deref(), Some("SyntaxSpec"));
    }

    #[test]
    fn canonical_contract_artifact_is_deterministic_and_serializable() {
        let artifact =
            cached_canonical_terlan_syntax_contract_artifact().expect("syntax contract artifact");
        let second =
            cached_canonical_terlan_syntax_contract_artifact().expect("syntax contract artifact");

        assert_eq!(artifact, second);
        assert_eq!(artifact.schema, SYNTAX_CONTRACT_ARTIFACT_SCHEMA);
        assert_eq!(
            artifact.fingerprint_algorithm,
            SYNTAX_CONTRACT_FINGERPRINT_ALGORITHM
        );
        assert!(artifact.fingerprint.starts_with("fnv1a64:"));
        assert_eq!(
            artifact.fingerprint,
            syntax_contract_fingerprint(&artifact.contract).expect("fingerprint")
        );

        let identity =
            cached_canonical_terlan_syntax_contract_identity().expect("syntax contract identity");
        assert_eq!(identity.schema, artifact.schema);
        assert_eq!(
            identity.fingerprint_algorithm,
            artifact.fingerprint_algorithm
        );
        assert_eq!(identity.fingerprint, artifact.fingerprint);
        assert_eq!(
            syntax_contract_identity_from_fingerprint(artifact.fingerprint.clone()),
            identity
        );
        assert!(
            syntax_contract_identity_matches_current(&identity).expect("identity matches current")
        );

        let old_identity = syntax_contract_identity_from_fingerprint("fnv1a64:0000000000000000");
        assert!(!syntax_contract_identity_matches_current(&old_identity)
            .expect("identity mismatch is checked"));

        let identity_json = cached_canonical_terlan_syntax_contract_identity_json()
            .expect("syntax contract identity json");
        let decoded_identity = serde_json::from_str::<SyntaxContractIdentity>(&identity_json)
            .expect("decode identity");
        assert_eq!(decoded_identity, identity);

        let json = cached_canonical_terlan_syntax_contract_artifact_json()
            .expect("syntax contract artifact json");
        let decoded =
            serde_json::from_str::<SyntaxContractArtifact>(&json).expect("decode artifact json");
        assert_eq!(decoded, artifact);
    }

    #[test]
    fn syntax_contract_artifact_matching_accepts_json_and_raw_fingerprint() {
        let artifact =
            cached_canonical_terlan_syntax_contract_artifact().expect("syntax contract artifact");
        let json = cached_canonical_terlan_syntax_contract_artifact_json()
            .expect("syntax contract artifact json");

        assert_eq!(
            extract_syntax_contract_artifact_fingerprint(&json),
            Some(artifact.fingerprint.clone())
        );
        assert_eq!(
            extract_syntax_contract_artifact_fingerprint(&format!("{}\n", artifact.fingerprint)),
            Some(artifact.fingerprint.clone())
        );
        assert!(syntax_contract_artifact_matches_current(&json).expect("match json"));
        assert!(
            syntax_contract_artifact_matches_current(&artifact.fingerprint)
                .expect("match fingerprint")
        );
        assert_eq!(
            check_syntax_contract_artifact_against_current(&json).expect("check json"),
            SyntaxContractArtifactCheck::Match {
                fingerprint: artifact.fingerprint.clone()
            }
        );
        assert_eq!(
            check_syntax_contract_artifact_against_current("fnv1a64:0000000000000000")
                .expect("check mismatch"),
            SyntaxContractArtifactCheck::Mismatch {
                expected: artifact.fingerprint.clone(),
                found: "fnv1a64:0000000000000000".to_string()
            }
        );
        assert_eq!(
            check_syntax_contract_artifact_against_current("{}").expect("check invalid"),
            SyntaxContractArtifactCheck::InvalidArtifact
        );
        assert!(
            !syntax_contract_artifact_matches_current("fnv1a64:0000000000000000")
                .expect("mismatch fingerprint")
        );
        assert!(extract_syntax_contract_artifact_fingerprint("{}").is_none());
        assert!(extract_syntax_contract_artifact_fingerprint(
            r#"{"fingerprint":"fnv1a64:bbc2bff7cdefae6c"}"#
        )
        .is_none());
        assert!(extract_syntax_contract_artifact_fingerprint(
            r#"{"schema":"other","fingerprint_algorithm":"fnv1a64","fingerprint":"fnv1a64:bbc2bff7cdefae6c"}"#
        )
        .is_none());
        assert!(extract_syntax_contract_artifact_fingerprint(
            r#"{"schema":"terlan-syntax-contract-v1","fingerprint_algorithm":"other","fingerprint":"fnv1a64:bbc2bff7cdefae6c"}"#
        )
        .is_none());
    }

    #[test]
    fn canonical_contract_artifact_matches_golden_summary() {
        let artifact =
            cached_canonical_terlan_syntax_contract_artifact().expect("syntax contract artifact");
        let actual = SyntaxContractArtifactSummary::from_artifact(&artifact);
        let expected = serde_json::from_str::<SyntaxContractArtifactSummary>(include_str!(
            "../../../docs/grammar/fixtures/contract/terlan_syntax_contract_artifact_summary.json"
        ))
        .expect("parse golden artifact summary");

        assert_eq!(actual, expected);
    }

    #[test]
    fn validator_rejects_broken_contract() {
        let mut contract =
            canonical_terlan_syntax_contract().expect("compile canonical syntax contract");
        contract.entry_rule = Some("Program".to_string());
        let expr_rule_index = contract
            .rules
            .iter()
            .position(|rule| rule.name == "Expr")
            .expect("Expr rule index");
        contract.rules[expr_rule_index].expr.kind = EbnfGrammarExprKind::Terminal {
            value: "broken".to_string(),
        };

        let diagnostics = validate_syntax_contract(&contract);
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("entry rule")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == "syntax rule Expr must reference AssignExpr"));
    }

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct SyntaxContractArtifactSummary {
        schema: String,
        fingerprint_algorithm: String,
        fingerprint: String,
        format_version: u32,
        entry_rule: String,
        rule_count: usize,
    }

    impl SyntaxContractArtifactSummary {
        fn from_artifact(artifact: &SyntaxContractArtifact) -> Self {
            Self {
                schema: artifact.schema.clone(),
                fingerprint_algorithm: artifact.fingerprint_algorithm.clone(),
                fingerprint: artifact.fingerprint.clone(),
                format_version: artifact.contract.format_version,
                entry_rule: artifact
                    .contract
                    .entry_rule
                    .clone()
                    .expect("canonical contract has entry rule"),
                rule_count: artifact.contract.rules.len(),
            }
        }
    }
}
