use super::{
    explicit_target_profile_override_error, infer_target_profile_from_typed_evidence,
    TargetInferenceInput, TargetProfile,
};

#[test]
fn target_profile_inference_defaults_target_neutral_code_to_vm() {
    let input = TargetInferenceInput::from_typed_evidence(&[], &[], &[]);

    let inference = infer_target_profile_from_typed_evidence(&input).expect("inference");

    assert_eq!(inference.profile, TargetProfile::Vm);
    assert_eq!(
        inference.reasons,
        vec!["no target-specific typed evidence; defaulting to vm"]
    );
}

#[test]
fn target_profile_inference_uses_vm_for_vm_native_http_and_db_evidence() {
    let input = TargetInferenceInput::from_typed_evidence(
        &[
            "std.vm.Agent",
            "std.native.collections.Vector",
            "std.http.Request",
            "std.db.Postgres",
        ],
        &["runtime.native.vector", "runtime.http.server"],
        &["target.vm.runtime"],
    );

    let inference = infer_target_profile_from_typed_evidence(&input).expect("inference");

    assert_eq!(inference.profile, TargetProfile::Vm);
    assert!(inference
        .reasons
        .iter()
        .any(|reason| reason == "import `std.db.Postgres` requires vm"));
}

#[test]
fn target_profile_inference_selects_shared_js_for_shared_js_imports() {
    let input = TargetInferenceInput::from_typed_evidence(&["std.js.Promise"], &[], &[]);

    let inference = infer_target_profile_from_typed_evidence(&input).expect("inference");

    assert_eq!(inference.profile, TargetProfile::JsShared);
    assert_eq!(
        inference.reasons,
        vec!["import `std.js.Promise` requires js.shared"]
    );
}

#[test]
fn target_profile_inference_selects_wasm_core_for_wasm_imports() {
    let input =
        TargetInferenceInput::from_typed_evidence(&["std.wasm.Abi"], &["runtime.wasm.core"], &[]);

    let inference = infer_target_profile_from_typed_evidence(&input).expect("inference");

    assert_eq!(inference.profile, TargetProfile::WasmCore);
    assert!(inference
        .reasons
        .iter()
        .any(|reason| reason == "import `std.wasm.Abi` requires wasm.core"));
}

#[test]
fn target_profile_inference_promotes_shared_js_to_browser_for_dom_evidence() {
    let input = TargetInferenceInput::from_typed_evidence(
        &["std.js.Promise", "std.js.Dom.Document"],
        &["runtime.js.dom.document"],
        &[],
    );

    let inference = infer_target_profile_from_typed_evidence(&input).expect("inference");

    assert_eq!(inference.profile, TargetProfile::JsBrowser);
    assert!(inference
        .reasons
        .iter()
        .any(|reason| reason == "import `std.js.Dom.Document` requires js.browser"));
}

#[test]
fn target_profile_inference_promotes_shared_js_to_worker_for_worker_evidence() {
    let input = TargetInferenceInput::from_typed_evidence(
        &["std.js.Promise", "std.js.Worker.GlobalScope"],
        &["runtime.js.worker.global"],
        &[],
    );

    let inference = infer_target_profile_from_typed_evidence(&input).expect("inference");

    assert_eq!(inference.profile, TargetProfile::JsWorker);
    assert!(inference
        .reasons
        .iter()
        .any(|reason| reason == "import `std.js.Worker.GlobalScope` requires js.worker"));
}

#[test]
fn target_profile_inference_rejects_vm_and_js_mixed_evidence() {
    let input =
        TargetInferenceInput::from_typed_evidence(&["std.vm.Agent", "std.js.Promise"], &[], &[]);

    let conflict = infer_target_profile_from_typed_evidence(&input).expect_err("conflict");

    assert_eq!(
        conflict.message,
        "target_ambiguous: typed target evidence requires both `vm` and `js.shared`"
    );
    assert_eq!(conflict.code, "target_ambiguous");
}

#[test]
fn target_profile_inference_rejects_browser_and_worker_mixed_evidence() {
    let input = TargetInferenceInput::from_typed_evidence(
        &["std.js.Dom.Document", "std.js.Worker.GlobalScope"],
        &[],
        &[],
    );

    let conflict = infer_target_profile_from_typed_evidence(&input).expect_err("conflict");

    assert_eq!(
        conflict.message,
        "target_ambiguous: typed target evidence requires both browser and worker JavaScript profiles"
    );
}

#[test]
fn target_profile_inference_reports_mixed_wasm_evidence_with_import_span() {
    let syntax = crate::terlan_syntax::parse_module_as_syntax_output(
        "module wasm.Mixed.\nimport std.wasm.Abi.{I32}.\nimport std.vm.Agent.\npub value(input: I32): I32 -> input.\n",
    )
    .expect("parse mixed target fixture");
    let expected_span = syntax.declarations[1].span;
    let input = TargetInferenceInput::from_syntax_modules([&syntax]);

    let conflict = infer_target_profile_from_typed_evidence(&input).expect_err("target conflict");

    assert_eq!(conflict.code, "target_ambiguous");
    assert_eq!(conflict.span, Some(expected_span));
    assert!(conflict.message.contains("at span"), "{}", conflict.message);
}

#[test]
fn target_profile_inference_requires_abi_namespace_for_local_scalar_alias() {
    let syntax = crate::terlan_syntax::parse_module_as_syntax_output(
        "module wasm.Missing.\npub value(input: I64): I64 -> input.\n",
    )
    .expect("parse missing ABI import fixture");
    let expected_span = match &syntax.declarations[0].payload {
        crate::terlan_syntax::SyntaxDeclarationPayload::Function { params, .. } => {
            params[0].annotation.span
        }
        _ => panic!("expected function"),
    };
    let input = TargetInferenceInput::from_syntax_modules([&syntax]);

    let conflict = infer_target_profile_from_typed_evidence(&input).expect_err("missing ABI");

    assert_eq!(conflict.code, "missing_abi_target");
    assert_eq!(conflict.span, Some(expected_span));
    assert!(conflict.message.contains("parameter `input`"));
}

#[test]
fn target_profile_inference_rejects_unsupported_abi_slot_with_exact_span() {
    let syntax = crate::terlan_syntax::parse_module_as_syntax_output(
        "module wasm.Unsupported.\nimport std.wasm.Abi.{I32}.\npub value(input: Binary): I32 -> 1.\n",
    )
    .expect("parse unsupported ABI fixture");
    let expected_span = match &syntax.declarations[1].payload {
        crate::terlan_syntax::SyntaxDeclarationPayload::Function { params, .. } => {
            params[0].annotation.span
        }
        _ => panic!("expected function"),
    };
    let input = TargetInferenceInput::from_syntax_modules([&syntax]);

    let conflict =
        infer_target_profile_from_typed_evidence(&input).expect_err("unsupported ABI slot");

    assert_eq!(conflict.code, "unsupported_abi_signature");
    assert_eq!(conflict.span, Some(expected_span));
    assert!(conflict.message.contains("type `Binary` is unsupported"));
}

#[test]
fn target_profile_inference_explicit_override_reports_family_conflict() {
    let input = TargetInferenceInput::from_typed_evidence(&["std.js.Promise"], &[], &[]);
    let inference = infer_target_profile_from_typed_evidence(&input).expect("inference");

    let error = explicit_target_profile_override_error(&inference, TargetProfile::Vm)
        .expect("override conflict");

    assert_eq!(
        error,
        "explicit target `vm` conflicts with typed target evidence for `js.shared`"
    );
}

#[test]
fn target_profile_inference_allows_explicit_override_for_target_neutral_code() {
    let input = TargetInferenceInput::from_typed_evidence(&[], &[], &[]);
    let inference = infer_target_profile_from_typed_evidence(&input).expect("inference");

    assert_eq!(
        explicit_target_profile_override_error(&inference, TargetProfile::JsShared),
        None
    );
}

#[test]
fn target_profile_inference_explicit_override_reports_narrowing_conflict() {
    let input = TargetInferenceInput::from_typed_evidence(&["std.js.Dom.Document"], &[], &[]);
    let inference = infer_target_profile_from_typed_evidence(&input).expect("inference");

    let error = explicit_target_profile_override_error(&inference, TargetProfile::JsShared)
        .expect("override conflict");

    assert_eq!(
        error,
        "explicit target `js.shared` cannot satisfy browser-only typed evidence"
    );
}
