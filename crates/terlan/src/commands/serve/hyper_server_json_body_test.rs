//! Source-owned library calls through the production protocol and actor path.

use super::*;

fn with_source_handler(source: &str, check: impl FnOnce(&dyn Fn(&str) -> String)) {
    let root = temp_web_root();
    let web = root.join("_build/web");
    std::fs::create_dir_all(root.join("src/app")).unwrap();
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"json_body_test\"\nversion = \"0.0.9\"\nnamespace = \"app\"\n",
    )
    .unwrap();
    std::fs::write(web.join("index.html"), "").unwrap();
    std::fs::write(root.join("src/app/Api.terl"), source).unwrap();
    std::fs::write(
        web.join("manifest.json"),
        r#"{
        "schema":"terlan-web-build-v1", "target_profile":"js.browser",
        "source_js_manifest":"../js/manifest.json", "index":"index.html", "assets":[],
        "handlers":[{"method":"POST","route":"/json","module":"app.Api",
            "function":"handle","arity":1,"source":{"path":"src/app/Api.terl","line":8,"column":5}}]
    }"#,
    )
    .unwrap();
    crate::commands::serve::prewarm_dynamic_handler_sources(&web)
        .expect("compile source-owned library handler");
    with_source_protocol_server(web, |send| {
        check(&|body| {
            send(&format!(
            "POST /json HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()
        ))
        });
    });
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn vm_stream_package_value_binding_uses_default_worker() {
    with_source_handler(
        r#"module app.Api.
import std.core.Atom.
import std.core.Result.{Ok, Err}.
import std.net.Uri.
import std.http.Response.
import type std.http.Request.{Request}.
import type std.http.Response.{Response}.
pub handle(request: Request): Response ->
    case Uri.parse(request.body_text()) {
        Ok(uri) -> Response.text(uri.scheme() + "|" + uri.path());
        Err(reason) -> Response.text(Atom.to_string(reason.code)).with_status(400)
    }.
"#,
        |request| {
            for (body, status, expected) in [
                ("https://example.com/a?x=1", 200, "https|/a"),
                ("mailto:user@example.com", 200, "mailto|user@example.com"),
                ("not a URL", 400, "uri.parse"),
                ("https://example.com/recovered", 200, "https|/recovered"),
            ] {
                let response = request(body);
                assert!(
                    response.starts_with(&format!("HTTP/1.1 {status}")),
                    "{response}"
                );
                assert!(
                    response.ends_with(&format!("\r\n\r\n{expected}")),
                    "{response}"
                );
            }
        },
    );
}

#[test]
fn vm_stream_request_decodes_managed_json_body_result() {
    with_source_handler(
        r#"module app.Api.
import std.core.{Atom, Int}.
import std.core.Result.{Ok, Err}.
import std.data.Json.
import std.http.Response.
import type std.http.Request.{Request}.
import type std.http.Response.{Response}.
pub handle(request: Request): Response ->
    case request.body_json() {
        Ok(json) -> case json.get("count") {
            Ok(value) -> case value.as_int() {
                Ok(count) -> Response.text(Int.to_string(count + 1)).with_status(201);
                Err(_) -> Response.text("not an integer").with_status(422)
            };
            Err(_) -> Response.text("no count").with_status(422)
        };
        Err(reason) -> Response.text(Atom.to_string(reason.code) + ":" + reason.message).with_status(400)
    }.
"#,
        |request| {
            for (body, status, expected) in [
                (r#"{"count":2}"#, 201, "3"),
                (r#"{"count":-1}"#, 201, "0"),
                (r#"{"count":"2"}"#, 422, "not an integer"),
                ("null", 422, "no count"),
                ("[]", 422, "no count"),
            ] {
                let response = request(body);
                assert!(
                    response.starts_with(&format!("HTTP/1.1 {status}")),
                    "{response}"
                );
                assert!(
                    response.ends_with(&format!("\r\n\r\n{expected}")),
                    "{response}"
                );
            }
            for body in ["", "{", "{\"count\":1,}", "{} trailing", "\0", "[1,]"] {
                let response = request(body);
                assert!(response.starts_with("HTTP/1.1 400"), "{body:?}: {response}");
                let (_, error) = response.split_once("\r\n\r\n").expect("HTTP response body");
                assert!(error.starts_with("http.body_json:"), "{error}");
                assert!(
                    error.len() > "http.body_json:".len(),
                    "parser message retained"
                );
            }
            let recovered = request(r#"{"count":41}"#);
            assert!(recovered.starts_with("HTTP/1.1 201"), "{recovered}");
            assert!(recovered.ends_with("\r\n\r\n42"), "{recovered}");
            // More than the worker-owner limit proves completed requests retire resources.
            for count in 0..20 {
                let response = request(&format!("{{\"count\":{count}}}"));
                assert!(response.starts_with("HTTP/1.1 201"), "{response}");
                assert!(
                    response.ends_with(&format!("\r\n\r\n{}", count + 1)),
                    "{response}"
                );
            }
        },
    );
}

#[test]
fn vm_stream_resource_mutations_keep_alias_identity_in_default_worker() {
    with_source_handler(
        r#"module app.Api.
import std.data.Json.
import std.http.Response.
import type std.http.Request.{Request}.
import type std.http.Response.{Response}.
pub handle(request: Request): Response ->
    let values = Json.array();
    let alias = values;
    values.push(Json.int(1));
    values.push(values);
    Response.text(Json.to_string(alias)).with_status(201).
"#,
        |request| {
            for _ in 0..3 {
                let response = request("");
                assert!(response.starts_with("HTTP/1.1 201"), "{response}");
                assert!(response.ends_with("\r\n\r\n[1,[1]]"), "{response}");
            }
        },
    );
}
