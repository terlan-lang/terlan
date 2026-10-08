use super::*;
use crate::commands::serve::handler_cache::invocation::AotHandlerInvocationStep;
use crate::runtime::vm::package_native_helper::VmPackageNativeHelpers;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

#[test]
fn source_middleware_stages_suspend_resume_cancel_and_short_circuit() {
    const MODULE: &str = "app.SourcePipeline";
    let fixture = compile_native_handler_fixture(
        "source_pipeline_suspension",
        "src/app/SourcePipeline.terl",
        "app_SourcePipeline",
        r#"module app.SourcePipeline.
import std.http.{Router, Response, Cookies}.
import std.http.Router.{Continue, Respond}.
import type std.http.Request.Request.
import type std.http.Response.Response.
import type std.http.Router.Router.
import type std.http.Router.MiddlewareResult.
first(label: String): MiddlewareResult -> Cookies.set_header("first", label); Continue.
stop(label: String): MiddlewareResult -> Respond(Response.text(Cookies.set_header("second", label))).
unreachable(label: String): MiddlewareResult -> Respond(Response.text(Cookies.set_header("unreachable", label))).
pub router(): Router ->
    let label = "captured";
    Router.new()
        .use((_request: Request) -> first(label))
        .use((_request: Request) -> stop(label))
        .use((_request: Request) -> unreachable(label))
        .map_response((_request: Request, response: Response) ->
            response.with_header("X-Outer", Cookies.set_header("outer", label)))
        .map_response((_request: Request, response: Response) ->
            response.with_header("X-Inner", Cookies.set_header("inner", label)))
        .get("/pipeline", (_request: Request) -> Response.text("handler")).
"#,
    );
    let runtime =
        AotHandlerRuntime::load_with_shard_count(MODULE.into(), &fixture.image, None, 1).unwrap();
    let router = runtime
        .execute_http_router(MODULE, "router", &mut |_| {})
        .unwrap();
    let RouterOutcome::Matched(route) = router.dispatch(RouteMethod::Get, "/pipeline").unwrap()
    else {
        panic!("compiled middleware route");
    };
    let [before] = route.middleware.as_slice() else {
        panic!("one source request stage")
    };
    let [post] = route.response_middleware.as_slice() else {
        panic!("one source response stage")
    };
    let input = request("/pipeline");
    let begin = || {
        runtime
            .begin_callable_invocation(MODULE, before, vec![input.clone()])
            .unwrap()
    };
    let mut helpers = VmPackageNativeHelpers::default();
    let mut resume = |step: AotHandlerInvocationStep, name: &str| {
        let AotHandlerInvocationStep::CapabilityWaiting(wait) = step else {
            panic!("source stage must park")
        };
        assert_eq!(wait.request().unwrap().operation, "std.http.cookies.encode");
        let ReplValue::String(header) = helpers.call(1, wait.request().unwrap(), &[]).unwrap()
        else {
            panic!("cookie codec result")
        };
        assert!(header.starts_with(&format!("{name}=captured")), "{header}");
        (
            wait.resume(NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Text(
                header.clone(),
            )))
            .unwrap(),
            header,
        )
    };

    // Dropping a parked continuation must not consume or mutate its captured stage.
    let (cancelled, _) = resume(begin(), "first");
    assert!(matches!(
        cancelled,
        AotHandlerInvocationStep::CapabilityWaiting(_)
    ));
    drop(cancelled);
    let (failed, _) = resume(begin(), "first");
    let AotHandlerInvocationStep::CapabilityWaiting(failed) = failed else {
        panic!("second callback must park")
    };
    assert!(failed
        .resume(NativeBoundaryReplyTerm::Error {
            code: "cookie.invalid".into(),
            message: "injected codec failure".into(),
            offset: 0,
        })
        .is_err());

    let (step, _) = resume(begin(), "first");
    let (step, second) = resume(step, "second");
    let AotHandlerInvocationStep::Complete(ReplValue::Record { name, mut fields }) = step else {
        panic!("Respond must short-circuit without the third native call");
    };
    assert_eq!(name, "Respond");
    assert_eq!(fields.len(), 1);
    let (field, response) = fields.pop().unwrap();
    assert_eq!(field, "response");
    let step = runtime
        .begin_callable_invocation(MODULE, post, vec![input, response])
        .unwrap();
    let (step, inner) = resume(step, "inner");
    let (step, outer) = resume(step, "outer");
    let AotHandlerInvocationStep::Complete(response) = step else {
        panic!("response stage must complete")
    };
    let response =
        crate::commands::serve::handler::decode_owned_response(response, &fixture.root).unwrap();
    assert_eq!(
        response.body.as_bytes().expect("finite response"),
        second.as_bytes()
    );
    assert_eq!(
        response.headers[2..],
        vec![("X-Inner".into(), inner), ("X-Outer".into(), outer)]
    );
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
