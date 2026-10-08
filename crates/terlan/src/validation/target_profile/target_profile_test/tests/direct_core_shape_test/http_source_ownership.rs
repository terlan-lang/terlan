use super::*;
use crate::terlan_typeck::CoreProofCoverage;

fn remote(module: &str, function: &str, arity: usize) -> CoreExpr {
    CoreExpr::RemoteCall {
        module: module.into(),
        function: function.into(),
        type_args: Vec::new(),
        args: vec![CoreExpr::Int(0); arity],
    }
}

#[test]
fn http_api_names_do_not_grant_vm_intrinsic_evidence() {
    let apis: &[(&str, &[&str])] = &[
        ("Error", &["new", "code", "message", "status"]),
        (
            "Request",
            &["method", "path", "header", "body_text", "body_json"],
        ),
        ("Cookies", &["get", "all", "set_header"]),
        (
            "Response",
            &[
                "json",
                "json_text",
                "text",
                "html",
                "file",
                "stream",
                "redirect",
            ],
        ),
        (
            "Session",
            &[
                "current",
                "get",
                "set",
                "delete",
                "rotate",
                "expire",
                "with_response",
            ],
        ),
        (
            "Sse",
            &[
                "data",
                "with_id",
                "with_name",
                "with_retry_ms",
                "response",
                "endpoint",
                "endpoint_with_keep_alive",
            ],
        ),
        ("WebSocket", &["text", "ping", "pong", "close", "endpoint"]),
        (
            "Router",
            &[
                "new",
                "get",
                "post",
                "put",
                "patch",
                "delete",
                "head",
                "options",
                "sse",
                "websocket",
                "use",
                "map_response",
                "fallback",
                "error",
                "overload",
                "lifecycle",
                "group",
            ],
        ),
    ];
    for (name, functions) in apis {
        // Error is also the unrelated core Error provider's short head. Only
        // the qualified HTTP identity is unambiguous in that case.
        let heads = if *name == "Error" {
            vec![format!("std.http.{name}")]
        } else {
            vec![
                format!("std.http.{name}"),
                (*name).to_owned(),
                format!("app.{name}"),
            ]
        };
        for head in heads {
            for function in *functions {
                for arity in 0..=16 {
                    let call = remote(&head, function, arity);
                    assert!(
                        !TargetProfile::CoreV0.allows_expr_shape(&call),
                        "{head}.{function}/{arity}"
                    );
                    assert!(
                        TargetProfile::Vm.allows_expr_shape(&call),
                        "normal VM source calls remain supported"
                    );
                }
            }
        }
    }
}

#[test]
fn http_receiver_names_do_not_grant_vm_intrinsic_evidence() {
    for method in [
        "method",
        "param",
        "query",
        "query_string",
        "header",
        "cookie",
        "cookies",
        "body_text",
        "body_json",
        "status",
        "with_status",
        "with_header",
        "set_cookie_header",
        "cookie_with_options",
        "with_cookie",
        "with_cookie_options",
        "delete_cookie",
        "with_deleted_cookie",
    ] {
        for arity in 0..=12 {
            let call = remote("__receiver__", method, arity);
            assert!(
                !TargetProfile::CoreV0.allows_expr_shape(&call),
                "{method}/{arity}"
            );
            let mutable = CoreExpr::MutableReceiverCall {
                receiver: Box::new(CoreExpr::Var("input".into())),
                method: method.into(),
                args: vec![CoreExpr::Int(0); arity],
                effects: crate::terlan_typeck::CoreEffectSet {
                    effects: Vec::new(),
                },
            };
            assert!(
                !TargetProfile::CoreV0.allows_expr_shape(&mutable),
                "mut {method}/{arity}"
            );
            assert!(TargetProfile::Vm.allows_expr_shape(&mutable));
        }
    }
}

#[test]
fn http_remote_calls_cannot_bypass_missing_preservation_evidence() {
    for (head, function, arity) in [
        ("Response", "text", 1),
        ("Session", "current", 1),
        ("Router", "get", 3),
    ] {
        let mut module = module_with_core_body(remote(head, function, arity));
        let body = &mut module.functions[0].clauses[0].body;
        body.checked_preservation_evidence = None;
        body.proof_coverage = CoreProofCoverage::RuntimeBoundary;
        body.remote = Some(head.into());
        let violations = target_profile_checks(&module, TargetProfile::CoreV0);
        assert!(
            violations
                .iter()
                .any(|violation| violation.message.contains("RuntimeBoundary")),
            "{head}: {violations:?}"
        );
        assert!(!violations.is_empty());
        assert!(target_profile_checks(&module, TargetProfile::Vm).is_empty());
    }
}

#[test]
fn resolved_source_calls_need_no_http_api_allowlist() {
    let module = module_with_core_body(CoreExpr::Call {
        function: "application_owned_response_builder".into(),
        type_args: Vec::new(),
        args: vec![CoreExpr::Binary("body".into())],
    });
    assert!(target_profile_checks(&module, TargetProfile::CoreV0).is_empty());
    assert!(target_profile_checks(&module, TargetProfile::Vm).is_empty());
}
