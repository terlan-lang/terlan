//! Source middleware unions reach dispatch without compiler-owned constructors.

use super::compile_native_handler_fixture;
use crate::commands::serve::handler_cache::invocation::AotHandlerInvocationStep;
use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::vm::ReplValue;
use terlan_http_native::routing::MiddlewareResult;

#[test]
fn compiled_middleware_preserves_continuation_and_response_payload() {
    let fixture = compile_native_handler_fixture(
        "source_middleware_value",
        "src/app/Policy.terl",
        "app_Policy",
        r#"module app.Policy.
import std.http.Router.{Continue, Respond, MiddlewareResult}.
import std.http.Response.
pub handle(blocked: Bool, message: String): MiddlewareResult ->
    if {
        blocked -> Respond(Response.text(message, 403).with_header("X-Policy", "source"));
        true -> Continue
    }.
"#,
    );
    let runtime =
        AotHandlerRuntime::load_with_shard_count("app.Policy".into(), &fixture.image, None, 1)
            .unwrap();
    for blocked in [false, true, false, true] {
        let step = runtime
            .begin_request_invocation(
                "app.Policy",
                "handle",
                vec![ReplValue::Bool(blocked), ReplValue::String("denied".into())],
            )
            .unwrap();
        let AotHandlerInvocationStep::Complete(value) = step else {
            panic!("ordinary middleware construction must complete without a capability");
        };
        match MiddlewareResult::from_value(value).unwrap() {
            MiddlewareResult::Continue => assert!(!blocked),
            MiddlewareResult::Respond(response) => {
                assert!(blocked);
                let response =
                    crate::commands::serve::handler::decode_owned_response(response, &fixture.root)
                        .unwrap();
                assert_eq!(response.status, 403);
                assert_eq!(
                    response.body.as_bytes().expect("finite response"),
                    b"denied"
                );
                assert_eq!(
                    response.headers[2..],
                    vec![("X-Policy".into(), "source".into())]
                );
            }
        }
    }
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
