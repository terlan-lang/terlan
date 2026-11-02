use super::*;

fn syntax() -> SyntaxModuleOutput {
    crate::terlan_syntax::parse_module_as_syntax_output(
        "module sample.\npub run(): Int -> pick(1).\n",
    )
    .unwrap()
}
fn body(module: &SyntaxModuleOutput) -> &SyntaxExprOutput {
    let SyntaxDeclarationPayload::Function { clauses, .. } = &module.declarations[0].payload else {
        panic!("function fixture")
    };
    &clauses[0].body
}

#[test]
fn conflicting_visits_cannot_publish_a_selected_target() {
    let module = syntax();
    let (_, observations) = capture(|| {
        let _module = super::module(&module);
        for name in ["first", "second"] {
            let _site = expression(body(&module));
            accepted(Some(serde_json::json!({"name":name})));
        }
    });
    assert_eq!(observations.calls.len(), 1);
    assert_eq!(observations.calls[0]["status"], "unavailable");
    assert!(observations.calls[0]["target"].is_null());
    assert_eq!(
        observations.calls[0]["reason"],
        "conflicting_resolution_visits"
    );
}

#[test]
fn unindexed_nested_expressions_cannot_assign_a_target_to_the_parent() {
    let module = syntax();
    let (_, observations) = capture(|| {
        let _module = super::module(&module);
        let _parent = expression(body(&module));
        let _child = expression(&body(&module).children[0]);
        accepted(Some(serde_json::json!({"name":"not_the_parent"})));
    });
    assert_eq!(observations.calls.len(), 1);
    assert_eq!(observations.calls[0]["status"], "unavailable");
}

#[test]
fn equal_visits_converge_and_interrupted_nested_capture_restores_state() {
    let module = syntax();
    let (_, observations) = capture(|| {
        let _module = super::module(&module);
        for _ in 0..2 {
            let _site = expression(body(&module));
            let interrupted = std::panic::catch_unwind(|| {
                capture(|| panic!("interrupted capture"));
            });
            assert!(interrupted.is_err());
            accepted(Some(serde_json::json!({"name":"selected"})));
        }
    });
    assert_eq!(observations.calls.len(), 1);
    assert_eq!(observations.calls[0]["status"], "resolved");
    assert!(ACTIVE.with(|cell| cell.borrow().is_none()));
}

#[test]
fn same_signature_declaration_origins_share_the_total_capture_bound() {
    let scheme = FunctionScheme {
        params: vec![Type::Int],
        ret: Type::Int,
        generic_params: Vec::new(),
        bounds: Vec::new(),
    };
    let (_, observations) = capture(|| {
        for offset in 0..4097 {
            register_local(
                "sample",
                "pick",
                Span {
                    start: offset,
                    end: offset + 1,
                },
                &scheme,
            );
        }
        assert_eq!(
            ACTIVE.with(|cell| cell.borrow().as_ref().unwrap().index.declaration_count),
            4096
        );
    });
    assert!(observations.truncated);
}
