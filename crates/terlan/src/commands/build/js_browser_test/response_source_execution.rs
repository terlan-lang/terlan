//! Generated manifests must execute providers, not recognize their names.

use super::*;
use crate::commands::serve::{handle_vm_stream_http1_request, prewarm_dynamic_handler_sources};

#[test]
fn generated_response_routes_execute_shadowed_providers_over_http1() {
    let root = temp_dir("response_provider_authority");
    let source_dir = root.join("src/app");
    let js_root = root.join("_build/js");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(js_root.join("modules")).unwrap();
    fs::write(js_root.join("modules/app.js"), "export {};\n").unwrap();
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"response_authority\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
    )
    .unwrap();
    fs::write(source_dir.join("Response.terl"), r#"module app.Response.
import std.http.Response.{text as make_text, html as make_html, redirect as make_redirect, file as make_file}.
import type std.http.Response.{Response}.
pub text(body: String, status: Int = 218): Response ->
    make_text("provider:" + body, status).with_header("X-Provider", "source").
pub html(body: String, status: Int = 219): Response -> make_html("<main>" + body + "</main>", status).
pub redirect(path: String, status: Int = 307): Response -> make_redirect("/provider" + path, status).
pub file(_path: String, status: Int = 206): Response ->
    make_file("actual.txt", status, "application/provider").with_header("X-Provider", "file").
"#).unwrap();
    let source_path = source_dir.join("Http.terl");
    fs::write(
        &source_path,
        r#"module app.Http.
import app.Response.
import std.http.Router.
import type std.http.Request.{Request}.
import type std.http.Response.{Response}.
import type std.http.Router.{Router}.
pub router(): Router ->
    Router.new().get("/text", text).get("/html", html)
        .get("/redirect", redirect).get("/file", file).
pub text(_request: Request): Response -> Response.text("body").
pub html(_request: Request): Response -> Response.html("body").
pub redirect(_request: Request): Response -> Response.redirect("/next").
pub file(_request: Request): Response -> Response.file("decoy.txt").
"#,
    )
    .unwrap();
    let contract = js_target_contract(TargetProfile::JsBrowser).unwrap();
    write_browser_package(
        &js_root,
        contract,
        &[module_artifact("app.Http", &source_path)],
        None,
        false,
    )
    .unwrap();
    let web = root.join("_build/web");
    fs::write(web.join("actual.txt"), "source selected file").unwrap();
    fs::write(web.join("decoy.txt"), "incorrect compiler shortcut").unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(web.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["handlers"].as_array().unwrap().len(), 4);
    assert_eq!(manifest["static_responses"], serde_json::json!([]));
    assert_eq!(manifest["file_responses"], serde_json::json!([]));
    prewarm_dynamic_handler_sources(&web).expect("compile generated source handlers");
    for (route, status, content_type, body, header) in [
        (
            "text",
            218,
            "text/plain; charset=utf-8",
            "provider:body",
            "x-provider: source",
        ),
        (
            "html",
            219,
            "text/html; charset=utf-8",
            "<main>body</main>",
            "cache-control: no-cache",
        ),
        (
            "redirect",
            307,
            "text/plain; charset=utf-8",
            "",
            "location: /provider/next",
        ),
        (
            "file",
            206,
            "application/provider",
            "source selected file",
            "x-provider: file",
        ),
    ] {
        let wire = format!("GET /{route} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        let response =
            String::from_utf8(handle_vm_stream_http1_request(&web, wire.as_bytes()).unwrap())
                .unwrap();
        assert!(
            response.starts_with(&format!("HTTP/1.1 {status} ")),
            "{response}"
        );
        let (headers, actual_body) = response.split_once("\r\n\r\n").unwrap();
        let headers = headers.to_ascii_lowercase();
        assert!(headers.lines().any(|line| line == header), "{response}");
        assert!(
            headers
                .lines()
                .any(|line| line == format!("content-type: {content_type}")),
            "{response}"
        );
        assert_eq!(actual_body, body);
    }
    fs::remove_dir_all(root).unwrap();
}
