use super::*;

#[test]
fn malformed_traceparent_is_absent_and_scope_restores() {
    let context = next_request_context(
        RequestContextDescriptor {
            service: "registry",
            route: "/packages/:name",
            module: "Registry",
            function: "show",
            release_id: "release-1",
            source_file: "src/Registry.terl",
            source_line: 12,
        },
        Some("malformed"),
    );
    assert!(context.trace.is_none());
    {
        let _scope = RequestContextScope::enter(context);
        assert!(REQUEST_CONTEXT.with(|current| current.borrow().is_some()));
    }
    assert!(REQUEST_CONTEXT.with(|current| current.borrow().is_none()));
}
