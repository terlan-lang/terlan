use super::*;

/// Verifies Effect execution has one effectful VM intrinsic identity.
///
/// Inputs:
/// - Canonical `std.core.Effect.run/1` and malformed registry lookups.
///
/// Output:
/// - Test passes when only `run/1` resolves and its CoreIR metadata records a
///   dynamic result plus VM effect execution.
///
/// Transformation:
/// - Keeps inert Effect construction separate from the runtime boundary at the
///   compiler-owned intrinsic registry.
#[test]
fn effect_run_maps_to_effectful_vm_intrinsic() {
    let expr = core_intrinsic_expr_from_parts(
        "std.core.Effect",
        "run",
        vec![CoreExpr::Tuple(vec![
            CoreExpr::Atom("effect".to_string()),
            CoreExpr::Int(7),
        ])],
        Span::new(4, 12),
    )
    .expect("Effect.run intrinsic");

    let CoreExpr::Intrinsic(call) = expr else {
        panic!("expected intrinsic expression")
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmEffectRun)
    );
    assert_eq!(call.return_type, CoreType::Dynamic);
    assert_eq!(call.effects, core_vm_effect_execution_set());
    assert!(call.contract_text().starts_with("Intrinsic(vm.effect.run;"));
    assert_eq!(core_primitive_intrinsic("std.core.Effect", "run", 0), None);
    assert_eq!(
        core_primitive_intrinsic("std.core.Effect", "value", 1),
        None
    );
}
