use super::*;

#[test]
fn canonical_contract_compiles_from_embedded_ebnf() {
    let contract = canonical_terlan_syntax_contract().expect("compile canonical syntax contract");

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
    let embedded = canonical_terlan_syntax_contract().expect("compile embedded syntax contract");
    let direct = compile_ebnf(CANONICAL_TERLAN_EBNF).expect("compile direct syntax contract");

    assert_eq!(embedded, direct);
}

#[test]
fn validator_accepts_canonical_contract() {
    let contract = canonical_terlan_syntax_contract().expect("compile canonical syntax contract");

    let diagnostics = validate_syntax_contract(&contract);
    assert!(
        diagnostics.is_empty(),
        "unexpected syntax contract diagnostics: {diagnostics:?}"
    );
}

#[test]
fn validated_canonical_contract_returns_checked_contract() {
    let contract =
        validated_canonical_terlan_syntax_contract().expect("validated canonical syntax contract");

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
    assert!(syntax_contract_identity_matches_current(&identity).expect("identity matches current"));

    let old_identity = syntax_contract_identity_from_fingerprint("fnv1a64:0000000000000000");
    assert!(!syntax_contract_identity_matches_current(&old_identity)
        .expect("identity mismatch is checked"));

    let identity_json = cached_canonical_terlan_syntax_contract_identity_json()
        .expect("syntax contract identity json");
    let decoded_identity =
        serde_json::from_str::<SyntaxContractIdentity>(&identity_json).expect("decode identity");
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
        syntax_contract_artifact_matches_current(&artifact.fingerprint).expect("match fingerprint")
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
    assert!(
        extract_syntax_contract_artifact_fingerprint(
            r#"{"schema":"other","fingerprint_algorithm":"fnv1a64","fingerprint":"fnv1a64:bbc2bff7cdefae6c"}"#
        )
        .is_none()
    );
    assert!(
        extract_syntax_contract_artifact_fingerprint(
            r#"{"schema":"terlan-syntax-contract-v1","fingerprint_algorithm":"other","fingerprint":"fnv1a64:bbc2bff7cdefae6c"}"#
        )
        .is_none()
    );
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
