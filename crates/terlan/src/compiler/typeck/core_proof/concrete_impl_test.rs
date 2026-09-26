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
