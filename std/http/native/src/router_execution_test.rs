use super::test_support::{response, router, Script};
use super::*;
use crate::admitted_response::AdmittedResponse;
use terlan_runtime_abi::{native_record, NativeValue as V};

fn run(router: &Router<V>, host: &mut Script) -> Result<AdmittedResponse, WebRouteError> {
    execute(
        router,
        RouteMethod::Get,
        "/item/{id:Int}",
        "/item/42",
        &[("initial".into(), "snapshot".into())],
        None,
        host,
    )
}

#[test]
fn callback_order_capture_arguments_and_reverse_response_unwind() {
    let mut host = Script::new([
        ("first", Ok(V::Atom("continue".into()))),
        (
            "second",
            Ok(V::Record {
                name: "Continue".into(),
                fields: vec![],
            }),
        ),
        ("handler", Ok(response("handler"))),
        ("inner", Ok(response("inner"))),
        ("outer", Ok(response("outer"))),
    ]);
    host.arity = Some(2);
    let result = run(
        &router(&["first", "second"], &["outer", "inner"], false),
        &mut host,
    )
    .unwrap();
    assert_eq!(result.body.as_bytes(), Some(b"outer".as_slice()));
    assert_eq!(
        host.calls[0].1,
        vec![vec![("initial".to_owned(), "snapshot".to_owned())].into()]
    );
    let captures: V = vec![("id".to_owned(), "42".to_owned())].into();
    assert_eq!(host.calls[2].1, vec![captures.clone(), V::Int(42)]);
    assert_eq!(host.calls[3].1, vec![captures.clone(), response("handler")]);
    assert_eq!(host.calls[4].1, vec![captures, response("inner")]);
    assert_eq!(host.admitted, 1);
}

#[test]
fn middleware_short_circuit_precedes_stale_binding_and_callable_checks() {
    let response = response("denied");
    let mut host = Script::new([
        ("auth", Ok(native_record!(Respond, { response }))),
        ("decorate", Ok(self::response("decorated"))),
    ]);
    host.arity = None;
    let result = execute(
        &router(&["auth", "unreachable"], &["decorate"], true),
        RouteMethod::Get,
        "/stale",
        "/item/42",
        &[],
        None,
        &mut host,
    )
    .unwrap();
    assert_eq!(result.body.as_bytes(), Some(b"decorated".as_slice()));
    assert!(host.replies.is_empty());
}

#[test]
fn errors_recover_without_replaying_response_middleware() {
    for failure in ["middleware", "handler", "response", "admission"] {
        let mut replies = vec![("before", Ok(V::Atom("continue".into())))];
        match failure {
            "middleware" => replies[0].1 = Err("middleware failed".into()),
            "handler" => replies.push(("handler", Err("handler failed".into()))),
            _ => replies.push(("handler", Ok(response("original")))),
        }
        if failure == "handler" {
            replies.push(("recover", Ok(response("recovered"))));
            replies.push(("after", Ok(response("decorated"))));
        } else if failure == "middleware" {
            replies.push(("recover", Ok(response("recovered"))));
        } else {
            replies.push((
                "after",
                if failure == "response" {
                    Err("response failed".into())
                } else {
                    Ok(V::Record {
                        name: "Response".into(),
                        fields: vec![],
                    })
                },
            ));
            replies.push(("recover", Ok(response("recovered"))));
        }
        let mut host = Script::new(replies);
        let result = run(&router(&["before"], &["after"], true), &mut host).unwrap();
        assert_eq!(
            result.body.as_bytes().unwrap(),
            if failure == "handler" {
                b"decorated"
            } else {
                b"recovered"
            }
        );
        assert!(host.replies.is_empty());
        assert_eq!(host.admitted, if failure == "admission" { 2 } else { 1 });
    }
}

#[test]
fn invalid_middleware_and_response_values_are_not_silently_admitted() {
    for response_stage in [false, true] {
        let replies = if response_stage {
            vec![("handler", Ok(response("ok"))), ("after", Ok(V::Unit))]
        } else {
            vec![("before", Ok(V::Unit))]
        };
        let mut host = Script::new(replies);
        let error = run(
            &router(
                if response_stage { &[] } else { &["before"] },
                if response_stage { &["after"] } else { &[] },
                false,
            ),
            &mut host,
        )
        .unwrap_err();
        assert!(
            error.contains(if response_stage {
                "expected Response"
            } else {
                "middleware"
            }),
            "{error}"
        );
        assert_eq!(host.admitted, 0);
    }
}

#[test]
fn recovery_failure_retains_original_error_and_invalid_recovery_is_terminal() {
    for stage in ["before", "handler", "after"] {
        let mut replies = Vec::new();
        if stage != "before" {
            replies.push(("before", Ok(V::Atom("continue".into()))));
        }
        if stage == "after" {
            replies.push(("handler", Ok(response("original"))));
        }
        replies.push((stage, Err("original failure".into())));
        replies.push(("recover", Err("recovery failed".into())));
        let mut host = Script::new(replies);
        let error = run(&router(&["before"], &["after"], true), &mut host).unwrap_err();
        assert!(error.contains("original failure") && error.contains("recovery failed"));
        assert!(host.replies.is_empty());
        assert_eq!(host.admitted, 0);
    }
    let mut host = Script::new([("before", Err("rejected".into())), ("recover", Ok(V::Unit))]);
    assert!(run(&router(&["before"], &[], true), &mut host).is_err());
    assert_eq!(host.calls.len(), 2);
    assert_eq!(host.admitted, 1);
}

#[test]
fn conversion_recovery_is_bounded_and_never_replays_completed_stages() {
    let mut host = Script::new([("handler", Ok(V::Unit)), ("recover", Ok(V::Unit))]);
    assert!(run(&router(&[], &[], true), &mut host).is_err());
    assert_eq!(host.calls.len(), 2);
    assert_eq!(host.admitted, 2);
    let mut host = Script::new([("handler", Err("original".into()))]);
    assert_eq!(
        run(&router(&[], &[], false), &mut host)
            .unwrap_err()
            .to_string(),
        "original"
    );
}

#[test]
fn prepared_responses_keep_source_middleware_and_cannot_change_binding() {
    for (method, path, valid) in [
        (RouteMethod::Get, "/item/{id:Int}", true),
        (RouteMethod::Post, "/item/{id:Int}", false),
        (RouteMethod::Get, "/stale", false),
    ] {
        let mut host = Script::new(if valid {
            vec![("after", Ok(response("decorated")))]
        } else {
            vec![]
        });
        let result = execute(
            &router(&[], &["after"], false),
            RouteMethod::Get,
            "/irrelevant_manifest_row",
            "/item/42",
            &[],
            Some(PreparedResponse {
                method,
                route_pattern: path.into(),
                value: response("folded"),
            }),
            &mut host,
        );
        if valid {
            assert_eq!(
                result.unwrap().body.as_bytes(),
                Some(b"decorated".as_slice())
            );
            assert_eq!(host.calls[0].1[1], response("folded"));
        } else {
            assert!(result.unwrap_err().contains("folded static route"));
            assert!(host.calls.is_empty());
        }
    }
}

#[test]
fn route_and_arity_failures_do_not_invoke_handlers_or_recovery() {
    for (declared, path, arity, expected) in [
        ("/stale", "/item/42", Some(1), "manifest route"),
        ("/item/{id:Int}", "/missing", Some(1), "did not match"),
        ("/item/{id:Int}", "/item/42", None, "not callable"),
        ("/item/{id:Int}", "/item/42", Some(0), "expects 0"),
        ("/item/{id:Int}", "/item/42", Some(3), "expects 3"),
    ] {
        let mut host = Script::new([]);
        host.arity = arity;
        let error = execute(
            &router(&[], &[], true),
            RouteMethod::Get,
            declared,
            path,
            &[],
            None,
            &mut host,
        )
        .unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(host.calls.is_empty());
    }
    let channels = Router::new()
        .scoped_target(
            RouteMethod::Get,
            "/events",
            RouteTarget::SseEndpoint(crate::channel_plan::SseEndpointPlan::new(1, 64).unwrap()),
            vec![],
            vec![],
        )
        .unwrap();
    let error = execute(
        &channels,
        RouteMethod::Get,
        "/events",
        "/events",
        &[],
        None,
        &mut Script::new([]),
    )
    .unwrap_err();
    assert!(error.contains("did not resolve to a source handler"));
}

#[test]
fn argument_builder_rejects_forged_typed_captures_and_unknown_callables() {
    let RouterOutcome::Matched(mut dispatch) = router(&[], &[], false)
        .dispatch(RouteMethod::Get, "/item/42")
        .unwrap()
    else {
        panic!("matched");
    };
    dispatch.route_params[0].1 = "not-an-integer".into();
    assert!(handler_arguments(V::Unit, &dispatch, Some(2), Script::scalar).is_err());
    assert!(handler_arguments(V::Unit, &dispatch, None, Script::scalar)
        .unwrap_err()
        .contains("not callable"));
    assert_eq!(
        handler_arguments(V::Unit, &dispatch, Some(1), Script::scalar).unwrap(),
        vec![V::Unit]
    );
}
