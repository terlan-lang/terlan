use super::{build_http_handler_source_diagnostic, capture_http_handler_failure_support_bundle};
use crate::runtime::vm::process::{VmProcessId, VmProcessSource};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn vm_http_handler_failure_support_bundle_replays_stable_request_metadata() {
    let request = http::Request::builder()
        .method("POST")
        .uri("/checkout?debug=true")
        .body("sensitive body should not be copied".to_string())
        .expect("request");
    let process = VmProcessId::from_raw_for_test(71);
    let handler_source = source("checkout");

    let bundle = capture_http_handler_failure_support_bundle(
        2026,
        process,
        handler_source.clone(),
        "http:handler:checkout:71",
        &request,
        "payment adapter unavailable",
    )
    .expect("support bundle");

    assert_eq!(bundle.process, process);
    assert_eq!(bundle.handler_source, handler_source);
    assert_eq!(bundle.request_method, "POST");
    assert_eq!(bundle.request_path, "/checkout");
    assert_eq!(
        bundle.request_body_bytes,
        "sensitive body should not be copied".len()
    );
    assert_eq!(bundle.failure, "payment adapter unavailable");
    assert_eq!(bundle.replay.scheduler_seed, 2026);
    assert!(bundle.replay.finished);
    assert_eq!(bundle.replay.steps.len(), 1);
    let step = &bundle.replay.steps[0];
    assert_eq!(step.process, process);
    assert_eq!(step.resource.handle, "http:handler:checkout:71");
    assert_eq!(step.operation, "http.handler.failure");
    assert_eq!(step.outcome, "POST /checkout: payment adapter unavailable");
    assert!(!step.outcome.contains("sensitive body"));

    for (resource, failure, expected) in [
        (
            "http:handler:checkout:71",
            " ",
            "VM HTTP handler failure cannot be empty",
        ),
        (
            " ",
            "payment adapter unavailable",
            "VM support-bundle resource handle cannot be empty",
        ),
    ] {
        let error = capture_http_handler_failure_support_bundle(
            2026,
            process,
            source("checkout"),
            resource,
            &request,
            failure,
        )
        .expect_err("invalid support-bundle metadata should fail closed");
        assert_eq!(error, expected);
    }
}

#[test]
fn vm_http_handler_source_diagnostic_preserves_source_map_identity() {
    let request = http::Request::builder()
        .method("GET")
        .uri("/users/42")
        .body("body must not be retained".to_string())
        .expect("request");
    let handler_source = source("show_user");

    let diagnostic = build_http_handler_source_diagnostic(
        "src/users.terl",
        &handler_source,
        &request,
        "handler returned invalid status",
    )
    .expect("diagnostic");

    assert_eq!(diagnostic.source_file, "src/users.terl");
    assert_eq!(diagnostic.module, "app.Main");
    assert_eq!(diagnostic.function, "show_user");
    assert_eq!(diagnostic.arity, 0);
    assert_eq!(diagnostic.request_method, "GET");
    assert_eq!(diagnostic.request_path, "/users/42");
    assert_eq!(diagnostic.message, "handler returned invalid status");
    assert!(!diagnostic.message.contains("body must not be retained"));

    let empty_source = build_http_handler_source_diagnostic(
        " ",
        &handler_source,
        &request,
        "handler returned invalid status",
    )
    .expect_err("source-linked diagnostics need a source file");
    assert_eq!(
        empty_source,
        "VM HTTP handler diagnostic source file cannot be empty"
    );

    let empty_message =
        build_http_handler_source_diagnostic("src/users.terl", &handler_source, &request, " ")
            .expect_err("diagnostic messages cannot be empty");
    assert_eq!(
        empty_message,
        "VM HTTP handler diagnostic message cannot be empty"
    );
}
