use super::*;

#[test]
fn parses_sampled_and_unsampled_w3c_context() {
    let sampled =
        TraceContext::parse_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
            .unwrap();
    assert!(sampled.sampled);
    assert!(
        !TraceContext::parse_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00")
            .unwrap()
            .sampled
    );
    assert!(TraceContext::parse_traceparent(
        "00-00000000000000000000000000000000-00f067aa0ba902b7-01"
    )
    .is_err());
}

fn request_context() -> RequestContext {
    RequestContext {
        service: "registry".into(),
        request_id: "request-1".into(),
        connection_id: "connection-1".into(),
        route_id: "package".into(),
        handler_id: "Registry.show".into(),
        release_id: "release-1".into(),
        actor_id: None,
        source: SourceIdentity {
            module: "Registry".into(),
            function: "show".into(),
            file: "src/Registry.terl".into(),
            line: 12,
        },
        trace: TraceContext::parse_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        )
        .ok(),
    }
}

#[test]
fn nested_cancelled_timed_out_and_detached_rules_are_deterministic() {
    let context = request_context();
    let nested = context
        .child("actor-2", ContextDisposition::Nested, WorkOutcome::Active)
        .unwrap();
    assert_eq!(nested.request_id, "request-1");
    assert!(nested.trace.is_some());
    assert!(context
        .child(
            "actor-3",
            ContextDisposition::Nested,
            WorkOutcome::Cancelled
        )
        .is_none());
    assert!(context
        .child("actor-4", ContextDisposition::Nested, WorkOutcome::TimedOut)
        .is_none());
    let detached = context
        .child(
            "actor-5",
            ContextDisposition::Detached,
            WorkOutcome::TimedOut,
        )
        .unwrap();
    assert!(detached.request_id.is_empty());
    assert!(detached.trace.is_none());
    assert_eq!(detached.release_id, "release-1");
}
