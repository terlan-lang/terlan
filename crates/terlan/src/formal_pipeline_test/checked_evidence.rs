/// Verifies checked external templates become CoreIR-owned render plans.
///
/// Inputs:
/// - One source module, template declaration, and external HTML template.
///
/// Output:
/// - The Core module retains the parsed tree, checked prop type, and lowered
///   default expression.
///
/// Transformation:
/// - Runs the complete formal pipeline and inspects only its CoreIR handoff.
#[test]
fn formal_pipeline_carries_validated_template_render_plans_into_core_ir() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan-core-template-plan-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("templates")).expect("create template fixture");
    let source_path = root.join("View.terl");
    std::fs::write(
        root.join("templates/page.terl.html"),
        "<main data-title=\"${title}\">${title}:${count * 2}</main>",
    )
    .expect("write template");
    let source = "module app.View.\n\nimport std.template.Template.\n\ntemplate Page from \"./templates/page.terl.html\" {\n    title: String = \"Default\",\n    count: Int = 7\n}.\n\npub page(): Template.Html ->\n    Page().\n";
    std::fs::write(&source_path, source).expect("write source");

    let artifacts = compile_syntax_module_through_phases_with_profile(
        &source_path.to_string_lossy(),
        source,
        DiagnosticFormat::Text {
            color: crate::ColorChoice::Never,
        },
        None,
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("compile template fixture");

    std::fs::remove_file(root.join("templates/page.terl.html"))
        .expect("remove checked template source");

    let [plan] = artifacts.core.templates.as_slice() else {
        panic!("expected one CoreIR template plan");
    };
    assert_eq!(plan.name, "Page");
    assert_eq!(plan.source_path, "./templates/page.terl.html");
    assert_eq!(plan.template.nodes.len(), 1);
    assert_eq!(plan.props.len(), 2);
    assert_eq!(plan.props[0].name, "title");
    assert_eq!(plan.props[0].ty, crate::terlan_typeck::CoreType::String);
    assert_eq!(
        plan.props[0].default,
        Some(crate::terlan_typeck::CoreExpr::Binary(
            "\"Default\"".to_string()
        ))
    );
    assert_eq!(plan.expressions.len(), 1);
    assert_eq!(plan.expressions[0].source, "count * 2");
    assert_eq!(plan.expressions[0].ty, crate::terlan_typeck::CoreType::Int);
    assert_eq!(
        plan.expressions[0].expr.contract_text(),
        "BinaryOp(*;Var(count), Int(2))"
    );

    std::fs::remove_dir_all(root).expect("cleanup");
}

/// Verifies termination obligations survive the complete checked pipeline.
#[test]
fn formal_pipeline_exposes_validated_termination_evidence() {
    let source = "module app.TerminationEvidence.\n\n\
        pub countdown(n: Int): Int ->\n\
            if { n > 0 -> countdown(n - 1); true -> 0 }.\n";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "/tmp/app/TerminationEvidence.terl",
        source,
        DiagnosticFormat::Text {
            color: crate::ColorChoice::Never,
        },
        None,
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("compile termination evidence fixture");
    crate::terlan_typeck::validate_core_termination_evidence(&artifacts.core)
        .expect("formal pipeline certificate");
    let proof = artifacts
        .core
        .termination
        .function("countdown", 1)
        .expect("countdown evidence");

    assert_eq!(
        proof.state,
        crate::terlan_typeck::CoreTerminationState::Proven
    );
    assert_eq!(
        proof.reason,
        crate::terlan_typeck::CoreTerminationReason::GuardedIntegerDescent
    );
    assert!(artifacts
        .core
        .contract_text()
        .contains("termination=countdown/1 state=Proven"));
}

/// Verifies lexical declarations and references survive the checked pipeline.
#[test]
fn formal_pipeline_exposes_validated_binding_identity_evidence() {
    let source = r#"
module app.BindingEvidence.

pub choose(value: Int, pair: {Int, Int}): Int ->
    case pair {
        {value, _} -> value;
        _ -> value
    }.
"#;
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "/tmp/app/BindingEvidence.terl",
        source,
        DiagnosticFormat::Text {
            color: crate::ColorChoice::Never,
        },
        None,
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("compile binding identity fixture");
    artifacts
        .core
        .binding_identities
        .validate()
        .expect("formal pipeline binding evidence");

    let values = artifacts
        .core
        .binding_identities
        .bindings
        .iter()
        .filter(|binding| binding.name == "value")
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 2, "{values:#?}");
    assert_ne!(values[0].id, values[1].id);
    assert_ne!(values[0].region, values[1].region);
    assert!(artifacts
        .core
        .binding_identities
        .references_for(values[0].id)
        .1
        .iter()
        .any(|reference| reference.name == "value"));
    assert!(artifacts
        .core
        .binding_identities
        .references_for(values[1].id)
        .1
        .iter()
        .any(|reference| reference.name == "value"));
}
use super::*;
