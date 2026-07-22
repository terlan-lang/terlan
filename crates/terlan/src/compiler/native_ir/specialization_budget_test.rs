//! Application-wide specialization budget and ordering tests.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, CoreModule};

use super::specialization_budget::{
    SpecializationBudget, SpecializationKind, MAX_APPLICATION_SPECIALIZATIONS,
};
use super::NativeModule;

/// Lowers one source module into checked CoreIR for application tests.
fn core_module(source: &str) -> CoreModule {
    let syntax = parse_module_as_syntax_output(source).expect("parse specialization fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

/// Proves all specialization passes consume one exact application ceiling.
#[test]
fn specialization_passes_share_one_exact_application_budget() {
    let quarter = MAX_APPLICATION_SPECIALIZATIONS / 4;
    let mut budget = SpecializationBudget::default();

    budget
        .reserve(SpecializationKind::Generic, "app.Alpha", quarter)
        .expect("reserve generic quarter");
    budget
        .reserve(SpecializationKind::HigherOrder, "app.Beta", quarter)
        .expect("reserve higher-order quarter");
    budget
        .reserve(SpecializationKind::StaticCallable, "app.Gamma", quarter)
        .expect("reserve static-callable quarter");
    budget
        .reserve(SpecializationKind::Projection, "app.Omega", quarter)
        .expect("reserve projection quarter");

    assert_eq!(budget.consumed(), MAX_APPLICATION_SPECIALIZATIONS);
    assert_eq!(
        budget
            .reserve(SpecializationKind::Generic, "app.Overflow", 1)
            .expect_err("reject expansion above application ceiling"),
        "error[native_ir.application_specialization_budget]: application specialization requested 513 expansions at `app.Overflow` during generic; maximum is 512"
    );
    assert_eq!(budget.consumed(), MAX_APPLICATION_SPECIALIZATIONS);
}

/// Proves application lowering and its output are independent of input order.
#[test]
fn application_specialization_uses_canonical_module_order() {
    let alpha = core_module("module app.Alpha.\n\npub value(): Int -> 1.\n");
    let omega = core_module("module app.Omega.\n\npub value(): Int -> 2.\n");

    let forward = NativeModule::lower_application(&[&alpha, &omega])
        .expect("lower forward application order");
    let reverse = NativeModule::lower_application(&[&omega, &alpha])
        .expect("lower reverse application order");

    assert_eq!(forward, reverse);
    assert_eq!(
        forward
            .iter()
            .map(|module| module.name.as_str())
            .collect::<Vec<_>>(),
        vec!["app.Alpha", "app.Omega"]
    );
}

/// Proves malformed duplicate module input fails before specialization.
#[test]
fn application_specialization_rejects_duplicate_modules_deterministically() {
    let module = core_module("module app.Duplicate.\n\npub value(): Int -> 1.\n");

    let error = NativeModule::lower_application(&[&module, &module])
        .expect_err("reject duplicate application module");

    assert_eq!(
        error,
        "error[native_ir.duplicate_module]: application contains duplicate module `app.Duplicate`"
    );
}
