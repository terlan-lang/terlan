//! Callback policy belongs to compiled package source, not syntax inspection.

use super::*;
use crate::commands::serve::{handle_vm_stream_http1_request, prewarm_dynamic_handler_sources};

#[test]
fn generated_router_executes_imported_and_captured_policy_over_http1() {
    let root = temp_dir("router_callback_authority");
    let source_dir = root.join("src/app");
    let js_root = root.join("_build/js");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(js_root.join("modules")).unwrap();
    fs::write(js_root.join("modules/app.js"), "export {};\n").unwrap();
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"callback_authority\"\nversion = \"0.0.9\"\nnamespace = \"app\"\n",
    )
    .unwrap();
    let policy = source_dir.join("Policy.terl");
    fs::write(&policy, r#"module app.Policy.
import std.http.{Request, Response, Error}.
import std.http.Router.{Continue, Respond}.
import type std.http.Request.{Request as Incoming}.
import type std.http.Response.{Response as Outgoing}.
import type std.http.Router.{MiddlewareResult as Decision}.
import type std.http.Error.{HttpError as Failure}.
pub gate(request: Incoming): Decision ->
    if { request.path() == "/blocked" -> Respond(Response.text("source-blocked", 403)); true -> Continue }.
pub decorate(_request: Incoming, response: Outgoing): Outgoing ->
    response.with_header("X-Policy", "imported").
pub recovery(prefix: String): (Failure) -> Outgoing ->
    (error: Failure) -> if {
        error.status() == 500 and error.code() == Atom["router_execution_failed"] -> Response.text(prefix + ":recovered", 502);
        true -> Response.text("invalid error projection", 599)
    }.
"#).unwrap();
    let source = source_dir.join("Http.terl");
    fs::write(
        &source,
        r#"module app.Http.
import app.Policy.{gate, decorate, recovery}.
import std.http.{Router, Response}.
import type std.http.Request.Request.
import type std.http.Response.Response.
import type std.http.Router.Router.
pub router(): Router ->
    let recover = recovery("captured");
    Router.new().use(gate).map_response(decorate)
        .get("/home", home).get("/blocked", home).get("/broken", broken).error(recover).
pub home(_request: Request): Response -> Response.text("source-home", 201).
pub broken(_request: Request): Response -> Response.file("../secret").
"#,
    )
    .unwrap();
    fs::write(js_root.join("modules/policy.js"), "export {};\n").unwrap();
    let mut policy_artifact = module_artifact("app.Policy", &policy);
    policy_artifact.relative_path = "modules/policy.js".into();
    write_browser_package(
        &js_root,
        js_target_contract(TargetProfile::JsBrowser).unwrap(),
        &[module_artifact("app.Http", &source), policy_artifact],
        None,
        false,
    )
    .expect("callback aliases and local values are ordinary source");
    let web = root.join("_build/web");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(web.join("manifest.json")).unwrap()).unwrap();
    assert!(manifest.get("error_handler").is_none());
    assert_eq!(manifest["handlers"].as_array().unwrap().len(), 3);
    prewarm_dynamic_handler_sources(&web).expect("admit source callback graph");
    for (path, status, body, decorated) in [
        ("/home", 201, "source-home", true),
        ("/blocked", 403, "source-blocked", true),
        ("/broken", 502, "captured:recovered", false),
    ] {
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        let response =
            String::from_utf8(handle_vm_stream_http1_request(&web, request.as_bytes()).unwrap())
                .unwrap();
        assert!(
            response.starts_with(&format!("HTTP/1.1 {status} ")),
            "{response}"
        );
        let (headers, actual) = response.split_once("\r\n\r\n").unwrap();
        assert_eq!(actual, body);
        assert_eq!(
            headers.to_ascii_lowercase().contains("x-policy: imported"),
            decorated,
            "{response}"
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn router_callback_types_are_checked_by_the_ordinary_compiler() {
    let cases = [
        ("use", "", false),
        (
            "use",
            "pub callback(_request: Request): String -> \"wrong\".",
            false,
        ),
        (
            "use",
            "pub callback(_request: String): MiddlewareResult -> Continue.",
            false,
        ),
        (
            "use",
            "pub callback(): MiddlewareResult -> Continue.",
            false,
        ),
        (
            "use",
            "pub callback(_request: Request): MiddlewareResult -> Continue.",
            true,
        ),
        ("map_response", "", false),
        (
            "map_response",
            "pub callback(_request: Request): Response -> Response.text(\"wrong\").",
            false,
        ),
        (
            "map_response",
            "pub callback(_request: Request, _response: Response): String -> \"wrong\".",
            false,
        ),
        (
            "map_response",
            "pub callback(_request: String, response: Response): Response -> response.",
            false,
        ),
        (
            "map_response",
            "pub callback(_request: Request, response: Response): Response -> response.",
            true,
        ),
        ("error", "", false),
        (
            "error",
            "pub callback(_error: String): Response -> Response.text(\"wrong\").",
            false,
        ),
        (
            "error",
            "pub callback(_error: HttpError): String -> \"wrong\".",
            false,
        ),
        (
            "error",
            "pub callback(): Response -> Response.text(\"wrong\").",
            false,
        ),
        (
            "error",
            "pub callback(_error: HttpError): Response -> Response.text(\"recovered\").",
            true,
        ),
    ];
    for (method, declaration, valid) in cases {
        let source = format!(
            r#"module app.CallbackCheck.
import std.http.{{Router, Response}}.
import std.http.Router.{{Continue}}.
import type std.http.Router.{{Router, MiddlewareResult}}.
import type std.http.Request.Request.
import type std.http.Response.Response.
import type std.http.Error.HttpError.
pub router(): Router -> Router.new().{method}(callback).get("/", home).
pub home(_request: Request): Response -> Response.text("home").
{declaration}
"#
        );
        for profile in [TargetProfile::Vm, TargetProfile::JsBrowser] {
            let result = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
                "callback_types.terl",
                &source,
                crate::DiagnosticFormat::Text {
                    color: crate::ColorChoice::Never,
                },
                None,
                crate::validation::native_policy::NativePolicy::NativeBoundaryOptional,
                profile,
            );
            assert_eq!(
                result.is_ok(),
                valid,
                "{profile:?}: {method}: {declaration}: {:?}",
                result.err()
            );
        }
    }
}
