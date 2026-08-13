use super::*;

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

/// Verifies browser package manifests serialize constant handlers as static responses.
///
/// Inputs:
/// - A fake JS build root with one emitted module file.
/// - A source module whose handlers return constant `Response.text` values.
///
/// Output:
/// - `_build/web/manifest.json` containing cacheable static response rows.
///
/// Transformation:
/// - Exercises the first static-response lowering pass so route manifests can
///   cache simple HTTP responses without invoking VM handlers.
#[test]
pub(super) fn write_browser_package_serializes_constant_handlers_as_static_responses() {
    let root = temp_dir("package_static_responses");
    let js_root = root.join("js");
    let modules_dir = js_root.join("modules");
    fs::create_dir_all(&modules_dir).expect("create modules dir");
    fs::write(modules_dir.join("app.js"), "export {};\n").expect("write js module");

    let source_path = root.join("Http.terl");
    write_router_source(&source_path);
    let modules = vec![module_artifact("app.Http", &source_path)];
    let contract = js_target_contract(TargetProfile::JsBrowser).expect("browser contract");

    write_browser_package(&js_root, contract, &modules, None, false).expect("write package");

    let manifest_text =
        fs::read_to_string(root.join("web/manifest.json")).expect("read web manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse web manifest");
    assert_eq!(manifest["handlers"].as_array().expect("handlers").len(), 0);
    let static_responses = manifest["static_responses"]
        .as_array()
        .expect("static responses");
    assert!(static_responses.iter().any(|response| {
        response["method"] == "GET"
            && response["route"] == "/"
            && response["module"] == "app.Http"
            && response["function"] == "home"
            && response["arity"] == 1
            && response["status"] == 200
            && response["content_type"] == "text/plain; charset=utf-8"
            && response["body"] == "home"
    }));
    assert!(static_responses.iter().any(|response| {
        response["method"] == "HEAD" && response["route"] == "*" && response["body"] == "not found"
    }));
    assert_eq!(static_responses.len(), 10);
    let home = static_responses
        .iter()
        .find(|response| response["method"] == "GET" && response["route"] == "/")
        .expect("home static response");
    assert_json_source(home, &source_path);

    fs::remove_dir_all(root).expect("cleanup package dir");
}

/// Verifies grouped constant routes reach the browser package manifest.
///
/// Inputs:
/// - A fake JS build root with one emitted module file.
/// - A source module whose grouped router handlers return constant
///   `Response.text` values.
///
/// Output:
/// - `_build/web/manifest.json` containing prefixed static response rows.
///
/// Transformation:
/// - Exercises the browser package writer boundary so grouped route lowering is
///   proven for the manifest consumed by `terlc serve`, not only the internal
///   route extractor.
#[test]
pub(super) fn write_browser_package_serializes_grouped_static_responses() {
    let root = temp_dir("package_grouped_static_responses");
    let js_root = root.join("js");
    let modules_dir = js_root.join("modules");
    fs::create_dir_all(&modules_dir).expect("create modules dir");
    fs::write(modules_dir.join("app.js"), "export {};\n").expect("write js module");

    let source_path = root.join("Http.terl");
    write_grouped_router_source(&source_path);
    let modules = vec![module_artifact("app.Http", &source_path)];
    let contract = js_target_contract(TargetProfile::JsBrowser).expect("browser contract");

    write_browser_package(&js_root, contract, &modules, None, false).expect("write package");

    let manifest_text =
        fs::read_to_string(root.join("web/manifest.json")).expect("read web manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse web manifest");
    assert_eq!(manifest["handlers"].as_array().expect("handlers").len(), 0);
    let static_responses = manifest["static_responses"]
        .as_array()
        .expect("static responses");
    assert!(static_responses.iter().any(|response| {
        response["method"] == "GET" && response["route"] == "/users" && response["body"] == "users"
    }));
    assert!(static_responses.iter().any(|response| {
        response["method"] == "GET"
            && response["route"] == "/users/:id"
            && response["body"] == "user"
    }));
    assert!(static_responses.iter().any(|response| {
        response["method"] == "HEAD"
            && response["route"] == "/users/*"
            && response["body"] == "missing"
    }));
    assert_eq!(static_responses.len(), 9);

    fs::remove_dir_all(root).expect("cleanup package dir");
}

/// Verifies browser package manifests serialize constant file handlers.
///
/// Inputs:
/// - A fake JS build root with one emitted module file.
/// - A source module whose handlers return constant `Response.file` values.
///
/// Output:
/// - `_build/web/manifest.json` containing route-backed file response rows.
///
/// Transformation:
/// - Exercises compiler-side file-response lowering so typed routes can stream
///   package files without invoking VM handlers.
#[test]
pub(super) fn write_browser_package_serializes_constant_handlers_as_file_responses() {
    let root = temp_dir("package_file_responses");
    let js_root = root.join("js");
    let modules_dir = js_root.join("modules");
    fs::create_dir_all(&modules_dir).expect("create modules dir");
    fs::write(modules_dir.join("app.js"), "export {};\n").expect("write js module");

    let source_path = root.join("Http.terl");
    write_file_router_source(&source_path);
    let modules = vec![module_artifact("app.Http", &source_path)];
    let contract = js_target_contract(TargetProfile::JsBrowser).expect("browser contract");

    write_browser_package(&js_root, contract, &modules, None, false).expect("write package");

    let manifest_text =
        fs::read_to_string(root.join("web/manifest.json")).expect("read web manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse web manifest");
    assert_eq!(manifest["handlers"].as_array().expect("handlers").len(), 0);
    assert_eq!(
        manifest["static_responses"]
            .as_array()
            .expect("static responses")
            .len(),
        0
    );
    let file_responses = manifest["file_responses"]
        .as_array()
        .expect("file responses");
    assert!(file_responses.iter().any(|response| {
        response["method"] == "GET"
            && response["route"] == "/download"
            && response["path"] == "downloads/report.txt"
            && response["status"] == 200
            && response["content_type"] == "text/plain; charset=utf-8"
    }));
    assert!(file_responses.iter().any(|response| {
        response["method"] == "GET"
            && response["route"] == "/manual"
            && response["path"] == "downloads/manual.pdf"
            && response["status"] == 206
            && response["content_type"] == "application/pdf"
    }));
    assert_eq!(file_responses.len(), 2);
    let download = file_responses
        .iter()
        .find(|response| response["method"] == "GET" && response["route"] == "/download")
        .expect("download file response");
    assert_json_source(download, &source_path);

    fs::remove_dir_all(root).expect("cleanup package dir");
}

/// Verifies browser package manifests serialize constant redirects.
///
/// Inputs:
/// - A fake JS build root with one emitted module file.
/// - A source module whose handler returns `Response.redirect`.
///
/// Output:
/// - `_build/web/manifest.json` containing a static response with `Location`.
///
/// Transformation:
/// - Exercises compiler-side redirect lowering so simple redirects are served
///   from the manifest without invoking VM handlers.
#[test]
pub(super) fn write_browser_package_serializes_constant_redirect_as_static_response() {
    let root = temp_dir("package_static_redirect");
    let js_root = root.join("js");
    let modules_dir = js_root.join("modules");
    fs::create_dir_all(&modules_dir).expect("create modules dir");
    fs::write(modules_dir.join("app.js"), "export {};\n").expect("write js module");

    let source_path = root.join("Http.terl");
    write_redirect_router_source(&source_path);
    let modules = vec![module_artifact("app.Http", &source_path)];
    let contract = js_target_contract(TargetProfile::JsBrowser).expect("browser contract");

    write_browser_package(&js_root, contract, &modules, None, false).expect("write package");

    let manifest_text =
        fs::read_to_string(root.join("web/manifest.json")).expect("read web manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse web manifest");
    assert_eq!(manifest["handlers"].as_array().expect("handlers").len(), 0);
    let static_responses = manifest["static_responses"]
        .as_array()
        .expect("static responses");
    assert_eq!(static_responses.len(), 1);
    let redirect = &static_responses[0];
    assert_eq!(redirect["method"], "GET");
    assert_eq!(redirect["route"], "/old");
    assert_eq!(redirect["status"], 301);
    assert_eq!(redirect["content_type"], "text/plain; charset=utf-8");
    assert_eq!(redirect["body"], "");
    assert_eq!(redirect["headers"][0]["name"], "Location");
    assert_eq!(redirect["headers"][0]["value"], "/new");

    fs::remove_dir_all(root).expect("cleanup package dir");
}

/// Verifies browser package manifests serialize router-level error handlers.
///
/// Inputs:
/// - A fake JS build root with one emitted module file.
/// - A source module with supported `Router.error` builder calls.
///
/// Output:
/// - `_build/web/manifest.json` containing the error handler row.
///
/// Transformation:
/// - Exercises the browser package writer boundary so error-handler discovery
///   is proven to affect the actual manifest consumed by `terlc serve`.
#[test]
pub(super) fn write_browser_package_serializes_router_error_handler() {
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
    let error_handler = &manifest["error_handler"];
    assert_eq!(error_handler["module"], "app.Http");
    assert_eq!(error_handler["function"], "render_error");
    assert_eq!(error_handler["arity"], 1);

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

/// Verifies route extraction rejects missing middleware functions.
///
/// Inputs:
/// - A source module whose router references `require_user` without declaring
///   it.
///
/// Output:
/// - Stable `error[web_router]` diagnostic.
///
/// Transformation:
/// - Exercises middleware validation before route manifest rows are
///   serialized.
#[test]
pub(super) fn discover_web_handlers_rejects_missing_middleware_function() {
    let source_path = temp_source_path("missing_middleware");
    write_invalid_middleware_source(&source_path, "");
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules).expect_err("missing middleware");

    assert!(error.contains("error[web_router]: middleware `require_user`"));
    assert!(error.contains("is not defined"));
    fs::remove_file(source_path).expect("cleanup router source");
}

/// Verifies route extraction rejects middleware with non-result returns.
///
/// Inputs:
/// - A source module whose router references a `Request -> String` middleware.
///
/// Output:
/// - Stable `error[web_router]` diagnostic.
///
/// Transformation:
/// - Covers the return-type half of middleware signature validation.
#[test]
pub(super) fn discover_web_handlers_rejects_wrong_middleware_return_type() {
    let source_path = temp_source_path("wrong_middleware_return");
    write_invalid_middleware_source(
        &source_path,
        "pub require_user(_request: Request): String ->\n    \"authorized\".\n",
    );
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules).expect_err("wrong middleware return");

    assert!(error.contains("error[web_router]: middleware `require_user`"));
    assert!(error.contains("must return MiddlewareResult, got `String`"));
    fs::remove_file(source_path).expect("cleanup router source");
}

/// Verifies route extraction rejects middleware with non-request params.
///
/// Inputs:
/// - A source module whose router references a `String -> MiddlewareResult`
///   middleware.
///
/// Output:
/// - Stable `error[web_router]` diagnostic.
///
/// Transformation:
/// - Covers the request-parameter half of middleware signature validation.
#[test]
pub(super) fn discover_web_handlers_rejects_wrong_middleware_request_type() {
    let source_path = temp_source_path("wrong_middleware_request");
    write_invalid_middleware_source(
        &source_path,
        "pub require_user(_request: String): MiddlewareResult ->\n    Atom[\"continue\"].\n",
    );
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules).expect_err("wrong middleware request");

    assert!(error.contains("error[web_router]: middleware `require_user`"));
    assert!(error.contains("must accept Request, got `String`"));
    fs::remove_file(source_path).expect("cleanup router source");
}

#[test]
pub(super) fn discover_web_handlers_rejects_missing_response_middleware_function() {
    let source_path = temp_source_path("missing_response_middleware");
    write_response_middleware_source(&source_path, "");
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules)
        .expect_err("missing response middleware should fail");

    assert!(
        error.contains("error[web_router]: response middleware `decorate`"),
        "{error}"
    );
    assert!(error.contains("is not defined"), "{error}");
    fs::remove_file(source_path).expect("cleanup router source");
}

#[test]
pub(super) fn discover_web_handlers_rejects_wrong_response_middleware_parameters() {
    let source_path = temp_source_path("wrong_response_middleware_parameters");
    write_response_middleware_source(
        &source_path,
        "pub decorate(_request: Request): Response ->\n    Response.text(\"wrong\").\n",
    );
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules)
        .expect_err("wrong response middleware parameters should fail");

    assert!(
        error.contains("error[web_router]: response middleware `decorate`"),
        "{error}"
    );
    assert!(
        error.contains("must accept Request and Response, got arity 1"),
        "{error}"
    );
    fs::remove_file(source_path).expect("cleanup router source");
}

#[test]
pub(super) fn discover_web_handlers_rejects_wrong_response_middleware_return_type() {
    let source_path = temp_source_path("wrong_response_middleware_return");
    write_response_middleware_source(
        &source_path,
        "pub decorate(_request: Request, _response: Response): String ->\n    \"wrong\".\n",
    );
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules)
        .expect_err("wrong response middleware return should fail");

    assert!(
        error.contains("error[web_router]: response middleware `decorate`"),
        "{error}"
    );
    assert!(
        error.contains("must return Response, got `String`"),
        "{error}"
    );
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
