//! Trait-family qualification must preserve proof debt and refresh its evidence.

use super::*;

fn lower(source: &str) -> CoreModule {
    let syntax = crate::terlan_syntax::parse_module_as_syntax_output(source).expect("parse");
    let resolved = crate::terlan_hir::resolve_syntax_module_output(&syntax).module;
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

#[test]
fn generic_trait_qualification_preserves_proof_model_requirement() {
    let core = lower("module generic_dispatch. pub trait Eq[A] { equal(left: A, right: A): Bool. }. pub compare[A](left: A, right: A)[Eq[A]]: Bool -> Eq.equal(left, right).");
    let function = core
        .functions
        .iter()
        .find(|function| function.name == "compare")
        .unwrap();
    let summary = &function.clauses[0].body;
    assert!(
        matches!(&summary.core_expr, Some(CoreExpr::Call { function, .. }) if function == "generic_dispatch.Eq.equal")
    );
    assert_eq!(
        summary.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert_eq!(
        summary.checked_preservation_evidence,
        summary
            .core_expr
            .as_ref()
            .and_then(core_expr_checked_preservation_evidence)
    );
}

#[test]
fn concrete_trait_rewrite_refreshes_preservation_target() {
    let core = lower("module concrete_dispatch. pub trait Eq[A] { equal(left: A, right: A): Bool. }. impl Eq[Int] for Int { equal(left: Int, right: Int): Bool -> left == right. }. pub compare(left: Int, right: Int): Bool -> Eq[Int].equal(left, right).");
    let function = core
        .functions
        .iter()
        .find(|function| function.name == "compare")
        .unwrap();
    let summary = &function.clauses[0].body;
    assert!(matches!(&summary.core_expr, Some(CoreExpr::Call { .. })));
    assert_eq!(
        summary.checked_preservation_evidence,
        summary
            .core_expr
            .as_ref()
            .and_then(core_expr_checked_preservation_evidence)
    );
}

/// Same-name implementation methods retain distinct identities and exact spans.
#[test]
fn concrete_trait_sources_survive_core_serialization() {
    let source = "module provenance. pub trait Value[T] { value(input: T): Int. }. impl Value[Int] for Int { value(input: Int): Int -> input. }. impl Value[Bool] for Bool { value(input: Bool): Int -> if { input -> 1; true -> 0 }. }.";
    let core = lower(source);
    let restored: CoreModule =
        serde_json::from_slice(&serde_json::to_vec(&core).expect("serialize CoreIR provenance"))
            .expect("restore CoreIR provenance");
    let mut identities = Vec::new();
    for function in restored
        .functions
        .iter()
        .filter(|function| function.trait_method.is_some())
    {
        let origin = function.source.as_ref().expect("implementation source");
        let span = origin.declaration_span.expect("parser-owned method span");
        let declaration = &source[span.start..span.end];
        assert!(declaration.starts_with("value(input:"), "{declaration}");
        assert!(
            declaration.contains(&function.params[0].ty),
            "{declaration}"
        );
        assert_eq!(origin.module, "provenance");
        assert_eq!(origin.arity, 1);
        identities.push(origin.function.clone());
    }
    identities.sort();
    assert_eq!(
        identities,
        [
            "provenance.Value[Bool].value",
            "provenance.Value[Int].value"
        ]
    );
}
