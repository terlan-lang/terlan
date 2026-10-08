use super::*;

#[test]
fn write_browser_manifest_rejects_route_namespace_conflicts_before_writing() {
    use serde_json::{from_value, json};
    for kind in ["handler", "websocket", "sse", "static", "file"] {
        let root = temp_dir(&format!("conflicting_manifest_{kind}"));
        let mut routes = WebRouteManifestRows::default();
        routes.handlers.push(
            from_value(json!({
                "method":"GET", "route":"/users/:id", "module":"app.Http",
                "function":"show", "arity":1
            }))
            .unwrap(),
        );
        let row = json!({
            "method":"GET", "route":"/users/:name", "module":"app.Http",
            "function":"show", "arity":1, "protocol":"chat.v1",
            "source":{"path":"app/Http.terl", "line":1, "column":1},
            "status":200, "content_type":"text/plain", "body":"hello", "path":"assets/hello.txt"
        });
        match kind {
            "handler" => routes.handlers.push(from_value(row).unwrap()),
            "websocket" => routes.websockets.push(from_value(row).unwrap()),
            "sse" => routes.sse.push(from_value(row).unwrap()),
            "static" => routes.static_responses.push(from_value(row).unwrap()),
            "file" => routes.file_responses.push(from_value(row).unwrap()),
            _ => unreachable!(),
        }
        let error = write_browser_manifest(
            &root,
            js_target_contract(TargetProfile::JsBrowser).unwrap(),
            Vec::new(),
            routes,
            None,
            false,
        )
        .expect_err("conflicting routes must not be published");
        assert!(error.contains("/users/:name"), "{error}");
        assert!(
            error.contains(if kind == "handler" {
                "duplicate or ambiguous handler route"
            } else {
                "conflicts with handler route `GET` `/users/:id`"
            }),
            "{error}"
        );
        assert!(!root.join("manifest.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn write_browser_manifest_rejects_package_invalid_route_metadata_before_writing() {
    use serde_json::{from_value, json};
    let cases = [
        (
            "handler",
            json!({"method":"GET","route":"/","module":"app.Http","function":"home","arity":0}),
        ),
        (
            "websocket",
            json!({"route":"/ws","protocol":"bad protocol"}),
        ),
        (
            "sse",
            json!({"module":"app.Events","route":"/events","source":{"path":"../bad","line":1,"column":1}}),
        ),
        (
            "static",
            json!({"method":"GET","route":"/","status":600,"content_type":"text/plain","body":"bad"}),
        ),
        (
            "file",
            json!({"method":"GET","route":"/file","path":"/outside","status":200}),
        ),
        (
            "error",
            json!({"module":"app.Http","function":"recover","arity":2}),
        ),
    ];
    for (kind, value) in cases {
        let root = temp_dir(&format!("invalid_manifest_{kind}"));
        let mut routes = WebRouteManifestRows::default();
        let mut error_handler = None;
        match kind {
            "handler" => routes.handlers.push(from_value(value).unwrap()),
            "websocket" => routes.websockets.push(from_value(value).unwrap()),
            "sse" => routes.sse.push(from_value(value).unwrap()),
            "static" => routes.static_responses.push(from_value(value).unwrap()),
            "file" => routes.file_responses.push(from_value(value).unwrap()),
            "error" => error_handler = Some(from_value(value).unwrap()),
            _ => unreachable!(),
        }
        let error = write_browser_manifest(
            &root,
            js_target_contract(TargetProfile::JsBrowser).unwrap(),
            Vec::new(),
            routes,
            error_handler,
            false,
        )
        .expect_err("package-invalid records must not be published");
        assert!(error.starts_with("error[serve_package]:"), "{error}");
        assert!(!root.join("manifest.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

/// Verifies browser manifests reject duplicate final asset paths.
///
/// Inputs:
/// - Two asset rows from different source-relative paths.
/// - The same final `web_relative_path` for both rows.
///
/// Output:
/// - Test passes when manifest serialization fails before writing
///   `_build/web/manifest.json`.
///
/// Transformation:
/// - Exercises the final asset graph safety check after all producers have
///   copied assets but before stale or ambiguous served paths can reach the VM.
#[test]
pub(super) fn write_browser_manifest_rejects_duplicate_web_asset_paths() {
    let root = temp_dir("duplicate_web_asset_paths");
    let web_root = root.join("web");
    fs::create_dir_all(&web_root).expect("create web root");
    let contract = js_target_contract(TargetProfile::JsBrowser).expect("browser contract");
    let assets = vec![
        WebAssetArtifact {
            module: "app.Main".to_string(),
            kind: "javascript-module".to_string(),
            source_relative_path: "modules/app.js".to_string(),
            web_relative_path: "assets/shared.js".to_string(),
            fingerprint: 1,
            integrity: "sha256-app".to_string(),
        },
        WebAssetArtifact {
            module: String::new(),
            kind: "static-asset".to_string(),
            source_relative_path: "assets/shared.js".to_string(),
            web_relative_path: "assets/shared.js".to_string(),
            fingerprint: 2,
            integrity: "sha256-static".to_string(),
        },
    ];

    let error = write_browser_manifest(
        &web_root,
        contract,
        assets,
        WebRouteManifestRows::default(),
        None,
        false,
    )
    .expect_err("duplicate asset paths must be rejected");

    assert!(error.contains("error[web_assets]: duplicate browser asset path"));
    assert!(
        !web_root.join("manifest.json").exists(),
        "duplicate asset path must fail before manifest write"
    );

    fs::remove_dir_all(root).expect("cleanup package dir");
}

/// Constant-looking calls remain executable source, including grouped routes,
/// named arguments, file responses, and redirects.
#[test]
pub(super) fn write_browser_package_preserves_response_handlers_as_source_calls() {
    type Fixture = (
        fn(&std::path::Path),
        &'static [(&'static str, &'static str, &'static str)],
        usize,
    );
    let fixtures: [Fixture; 4] = [
        (
            write_router_source,
            &[("GET", "/", "home"), ("HEAD", "*", "not_found")],
            10,
        ),
        (
            write_grouped_router_source,
            &[
                ("GET", "/users", "users"),
                ("GET", "/users/:id", "show_user"),
                ("HEAD", "/users/*", "users_not_found"),
            ],
            9,
        ),
        (
            write_file_router_source,
            &[
                ("GET", "/download", "download"),
                ("GET", "/manual", "manual"),
            ],
            2,
        ),
        (write_redirect_router_source, &[("GET", "/old", "old")], 1),
    ];
    for (index, (write_source, expected, count)) in fixtures.into_iter().enumerate() {
        let root = temp_dir(&format!("package_source_responses_{index}"));
        let js_root = root.join("js");
        fs::create_dir_all(js_root.join("modules")).unwrap();
        fs::write(js_root.join("modules/app.js"), "export {};\n").unwrap();
        let source_path = root.join("Http.terl");
        write_source(&source_path);
        let modules = vec![module_artifact("app.Http", &source_path)];
        let contract = js_target_contract(TargetProfile::JsBrowser).unwrap();
        write_browser_package(&js_root, contract, &modules, None, false).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("web/manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["static_responses"], serde_json::json!([]));
        assert_eq!(manifest["file_responses"], serde_json::json!([]));
        let handlers = manifest["handlers"].as_array().unwrap();
        assert_eq!(handlers.len(), count);
        for &(method, route, function) in expected {
            let handler = handlers
                .iter()
                .find(|handler| handler["method"] == method && handler["route"] == route)
                .expect("source handler");
            assert_eq!(handler["module"], "app.Http");
            assert_eq!(handler["function"], function);
            assert_eq!(handler["arity"], 1);
            assert_json_source(handler, &source_path);
        }
        fs::remove_dir_all(root).unwrap();
    }
}

/// Recovery remains in executed source, not a compiler-synthesized callback row.
#[test]
pub(super) fn write_browser_package_keeps_error_handlers_in_source() {
    let root = temp_dir("package_error_handler");
    let js_root = root.join("js");
    let modules_dir = js_root.join("modules");
    fs::create_dir_all(&modules_dir).expect("create modules dir");
    fs::write(modules_dir.join("app.js"), "export {};\n").expect("write js module");

    let source_path = root.join("Http.terl");
    write_error_router_source(&source_path);
    let modules = vec![module_artifact("app.Http", &source_path)];
    let contract = js_target_contract(TargetProfile::JsBrowser).expect("browser contract");

    write_browser_package(&js_root, contract, &modules, None, false).expect("write package");

    let manifest_text =
        fs::read_to_string(root.join("web/manifest.json")).expect("read web manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse web manifest");
    assert!(manifest.get("error_handler").is_none());

    fs::remove_dir_all(root).expect("cleanup package dir");
}

/// Verifies route extraction rejects missing local handler functions.
///
/// Inputs:
/// - A source module whose router references `home` without declaring it.
///
/// Output:
/// - Stable `error[web_router]` diagnostic.
///
/// Transformation:
/// - Exercises the route-manifest extraction validation before manifest rows
///   are serialized.
#[test]
pub(super) fn discover_web_handlers_rejects_missing_handler_function() {
    let source_path = temp_source_path("missing_handler");
    write_invalid_router_source(&source_path, "");
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules).expect_err("missing handler");

    assert!(error.contains("error[web_router]: handler `home`"));
    assert!(error.contains("is not defined"));
    fs::remove_file(source_path).expect("cleanup router source");
}

/// Verifies route extraction rejects handlers with the wrong arity.
///
/// Inputs:
/// - A source module whose router references a zero-arity `home`.
///
/// Output:
/// - Stable `error[web_router]` diagnostic.
///
/// Transformation:
/// - Checks the local signature validation used before browser manifest
///   serialization.
#[test]
pub(super) fn discover_web_handlers_rejects_wrong_handler_arity() {
    let source_path = temp_source_path("wrong_handler_arity");
    write_invalid_router_source(
        &source_path,
        "pub home(): Response ->\n    Response.text(\"home\").\n",
    );
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules).expect_err("wrong arity");

    assert!(error.contains("error[web_router]: handler `home`"));
    assert!(error.contains("must accept Request or Request plus 0 route parameter(s), got arity 0"));
    fs::remove_file(source_path).expect("cleanup router source");
}

/// Verifies route extraction rejects handlers with non-response returns.
///
/// Inputs:
/// - A source module whose router references a `Request -> String` handler.
///
/// Output:
/// - Stable `error[web_router]` diagnostic.
///
/// Transformation:
/// - Covers the return-type half of the local handler signature validation used
///   by browser manifest generation.
#[test]
pub(super) fn discover_web_handlers_rejects_wrong_handler_return_type() {
    let source_path = temp_source_path("wrong_handler_return");
    write_invalid_router_source(
        &source_path,
        "pub home(_request: Request): String ->\n    \"home\".\n",
    );
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules).expect_err("wrong return type");

    assert!(error.contains("error[web_router]: handler `home`"));
    assert!(error.contains("must return Response, got `String`"));
    fs::remove_file(source_path).expect("cleanup router source");
}

/// Verifies route extraction rejects handlers with non-request first params.
///
/// Inputs:
/// - A source module whose router references a `String -> Response` handler.
///
/// Output:
/// - Stable `error[web_router]` diagnostic.
///
/// Transformation:
/// - Covers the request-parameter half of the local handler signature
///   validation used by browser manifest generation.
#[test]
pub(super) fn discover_web_handlers_rejects_wrong_handler_request_type() {
    let source_path = temp_source_path("wrong_handler_request");
    write_invalid_router_source(
        &source_path,
        "pub home(_request: String): Response ->\n    Response.text(\"home\").\n",
    );
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules).expect_err("wrong request type");

    assert!(error.contains("error[web_router]: handler `home`"));
    assert!(error.contains("must accept Request as parameter 1, got `String`"));
    fs::remove_file(source_path).expect("cleanup router source");
}

/// Verifies route extraction rejects malformed router paths.
///
/// Inputs:
/// - A source module whose router uses a non-final wildcard route.
///
/// Output:
/// - Stable `error[web_router]` diagnostic.
///
/// Transformation:
/// - Reuses the same route-pattern validation as `terlc serve` before browser
///   manifest serialization can write an invalid handler route.
#[test]
pub(super) fn discover_web_handlers_rejects_invalid_route_pattern() {
    let source_path = temp_source_path("invalid_route_pattern");
    write_invalid_route_source(&source_path, "/assets/*/tail");
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules).expect_err("invalid route");

    assert!(error.contains("error[web_router]: wildcard in handler route `/assets/*/tail`"));
    assert!(error.contains("must be the final segment"));
    fs::remove_file(source_path).expect("cleanup router source");
}

/// Verifies route extraction rejects ambiguous source route sets.
///
/// Inputs:
/// - A source module with two same-method parameter routes of the same shape.
///
/// Output:
/// - Stable `error[web_router]` diagnostic.
///
/// Transformation:
/// - Validates the full discovered handler set before browser manifest
///   serialization so `terlc build` catches ambiguity as early as `serve`.
#[test]
pub(super) fn discover_web_handlers_rejects_ambiguous_route_shapes() {
    let source_path = temp_source_path("ambiguous_routes");
    write_ambiguous_route_source(&source_path);
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules).expect_err("ambiguous route");

    assert!(error.contains("error[web_router]: duplicate or ambiguous handler route"));
    assert!(error.contains("GET"));
    assert!(error.contains("/users/:name"));
    fs::remove_file(source_path).expect("cleanup router source");
}
