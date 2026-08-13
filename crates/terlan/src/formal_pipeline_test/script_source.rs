use super::*;

/// Verifies `.terls` syntax reaches the maintained checked VM pipeline.
#[test]
fn script_source_synthesizes_a_typed_core_entrypoint() {
    let source = "answer = 40 + 2;\nassert_equal(answer, 42);\nanswer.\n";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "/tmp/scripts/Answer.terls",
        source,
        DiagnosticFormat::Text {
            color: crate::ColorChoice::Never,
        },
        None,
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("compile script through checked VM phases");

    assert_eq!(artifacts.syntax_output.module_name, "scripts.Answer");
    assert_eq!(artifacts.core.module, "scripts.Answer");
    let main = artifacts
        .core
        .functions
        .iter()
        .find(|function| function.name == "main" && function.arity == 0)
        .expect("synthetic main/0");
    assert_eq!(main.return_type, "Dynamic");
    assert_eq!(
        main.core_return_type,
        Some(crate::terlan_typeck::CoreType::Dynamic)
    );
}
