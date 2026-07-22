#[test]
fn vm_stream_request_dispatches_pattern_head_route_handler_without_hyper() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_pattern_head_route_handler");
    let project_root = &dir;
    let web_root = project_root.join("_build/web");
    let source_dir = project_root.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(
        project_root.join("terlan.toml"),
        "[package]\nname = \"serve_vm_stream_pattern_head_demo\"\nversion = \"0.0.7\"\n",
    )
    .expect("write project manifest");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        source_dir.join("Api.terl"),
        "module app.Api.\n\nimport std.collections.Map.\nimport std.core.Option.\nimport std.http.Response.\nimport type std.http.Response.{Response}.\n\npub show(\n    {Atom[\"request\"], _method, _path, params, _body, _query, _query_pairs, _headers, _cookies}: {Atom[\"request\"], String, String, Map[String, String], String, String, Map[String, String], Map[String, String], Map[String, String]},\n    id: String,\n    name: String\n): Response ->\n    Response.text(Option.with_default(params.get(\"id\"), \"missing\") + \":\" + id + \":\" + name).with_status(222).\n",
    )
    .expect("write handler source");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    {
      "method": "GET",
      "route": "/api/users/{id:Int}/files/:name",
      "module": "app.Api",
      "function": "show",
      "arity": 3,
      "source": {
        "path": "src/app/Api.terl",
        "line": 8,
        "column": 5
      }
    }
  ],
  "assets": [
    {
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    }
  ]
}
"#,
    )
    .expect("write manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm handler cache");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /api/users/42/files/read%20me HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream pattern-head route request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 222 "), "{response}");
    assert!(response.contains("content-type: text/plain; charset=utf-8\r\n"));
    assert!(response.ends_with("\r\n\r\n42:42:read me"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

#[test]
fn vm_stream_request_reports_pattern_head_route_handler_miss_without_hyper() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_pattern_head_route_handler_miss");
    let project_root = &dir;
    let web_root = project_root.join("_build/web");
    let source_dir = project_root.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(
        project_root.join("terlan.toml"),
        "[package]\nname = \"serve_vm_stream_pattern_head_miss_demo\"\nversion = \"0.0.7\"\n",
    )
    .expect("write project manifest");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        source_dir.join("Api.terl"),
        "module app.Api.\n\nimport std.collections.Map.\nimport std.http.Response.\nimport type std.http.Response.{Response}.\n\npub show(\n    {Atom[\"not_request\"], _method, _path, _params, _body, _query, _query_pairs, _headers, _cookies}: {Atom[\"not_request\"], String, String, Map[String, String], String, String, Map[String, String], Map[String, String], Map[String, String]},\n    _id: String,\n    _name: String\n): Response ->\n    Response.text(\"unreachable\").with_status(299).\n",
    )
    .expect("write handler source");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    {
      "method": "GET",
      "route": "/api/users/{id:Int}/files/:name",
      "module": "app.Api",
      "function": "show",
      "arity": 3,
      "source": {
        "path": "src/app/Api.terl",
        "line": 7,
        "column": 5
      }
    }
  ],
  "assets": [
    {
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    }
  ]
}
"#,
    )
    .expect("write manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm handler cache");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /api/users/42/files/read%20me HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream pattern-head route miss request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 502 "), "{response}");
    assert!(response.contains("content-type: text/plain; charset=utf-8\r\n"));
    assert!(
        response.contains("no function clause matched show/3"),
        "{response}"
    );
    assert!(response.contains("event=pattern_head_failed"), "{response}");
    assert!(response.contains("pattern_kind=Tuple"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

#[test]
fn vm_stream_head_request_falls_back_to_get_dynamic_handler_without_body() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_dynamic_handler_head_fallback");
    let project_root = &dir;
    let web_root = project_root.join("_build/web");
    let source_dir = project_root.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(
        project_root.join("terlan.toml"),
        "[package]\nname = \"serve_vm_stream_head_demo\"\nversion = \"0.0.7\"\n",
    )
    .expect("write project manifest");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        source_dir.join("Api.terl"),
        "module app.Api.\n\nimport std.http.Response.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\npub show(_request: Request): Response ->\n    Response.text(\"head fallback body\").with_status(219).\n",
    )
    .expect("write handler source");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    {
      "method": "GET",
      "route": "/api/head",
      "module": "app.Api",
      "function": "show",
      "arity": 1,
      "source": {
        "path": "src/app/Api.terl",
        "line": 7,
        "column": 5
      }
    }
  ],
  "assets": [
    {
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    }
  ]
}
"#,
    )
    .expect("write manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm handler cache");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"HEAD /api/head HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream HEAD dynamic fallback request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 219 "), "{response}");
    assert!(response.contains("content-type: text/plain; charset=utf-8\r\n"));
    assert!(response.ends_with("\r\n\r\n"));
    assert!(!response.contains("head fallback body"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

#[test]
fn vm_stream_request_uses_source_bridge_for_managed_only_module_without_hyper() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_managed_source_handler");
    let project_root = &dir;
    let web_root = project_root.join("_build/web");
    let source_dir = project_root.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(
        project_root.join("terlan.toml"),
        "[package]\nname = \"serve_vm_stream_managed_source_demo\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
    )
    .expect("write project manifest");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        source_dir.join("Api.terl"),
        "module app.Api.\n\nimport std.http.Response.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\npub handle(_request: Request): Response ->\n    Response.text(\"artifact handler\").with_status(209).\n",
    )
    .expect("write artifact handler source");

    let status = crate::commands::build::run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_dir.join("Api.terl").display().to_string(),
                "--target".to_string(),
                "terlan-vm".to_string(),
            ],
        },
        CliState {
            out_dir: web_root.clone(),
            ..CliState::default()
        },
    );
    assert_eq!(status, ExitCode::SUCCESS);
    assert!(web_root.join("vm/app_Api.tvm").exists());

    fs::write(
        source_dir.join("Api.terl"),
        "module app.Api.\n\nimport std.http.Request.\nimport std.http.Response.\nimport type std.core.Option.{Option}.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\npub handle(request: Request): Response ->\n    Response.text(request.query_string())\n        .with_status(210)\n        .with_header(\"Set-Cookie\", \"first=one\")\n        .with_header(\"Set-Cookie\", \"second=two\")\n        .with_cookie(\"session\", \"abc123\", \"/\", true, true)\n        .with_security_headers(Response.production_security_headers()).\n\nlookup(request: Request): Option[String] ->\n    request.query(\"page\").\n",
    )
    .expect("rewrite handler source after artifact build");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    {
      "method": "GET",
      "route": "/api/source",
      "module": "app.Api",
      "function": "handle",
      "arity": 1,
      "source": {
        "path": "src/app/Api.terl",
        "line": 7,
        "column": 5
      }
    }
  ],
  "assets": [
    {
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    }
  ]
}
"#,
    )
    .expect("write managed source handler manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm managed source handler cache");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /api/source?page=2 HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream managed source handler request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 210 "), "{response}");
    assert!(response.contains("content-type: text/plain; charset=utf-8\r\n"));
    assert!(response.contains("set-cookie: first=one\r\n"), "{response}");
    assert!(response.contains("set-cookie: second=two\r\n"), "{response}");
    assert!(
        response.contains("set-cookie: session=abc123; HttpOnly; Secure; Path=/\r\n"),
        "{response}"
    );
    assert_eq!(response.matches("set-cookie:").count(), 3, "{response}");
    assert!(response.contains("x-frame-options: DENY\r\n"), "{response}");
    assert!(
        response.contains("referrer-policy: strict-origin-when-cross-origin\r\n"),
        "{response}"
    );
    assert!(
        response.contains("x-content-type-options: nosniff\r\n"),
        "{response}"
    );
    assert!(
        response.contains(
            "strict-transport-security: max-age=31536000; includeSubDomains\r\n"
        ),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\npage=2"));

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

#[test]
fn vm_stream_request_reports_dynamic_handler_runtime_error_without_hyper() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_dynamic_handler_runtime_error");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    fs::write(
        dir.join("terlan.toml"),
        "[package]\nname = \"serve_vm_stream_stale_demo\"\nversion = \"0.0.7\"\n",
    )
    .expect("write project manifest");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    {
      "method": "GET",
      "route": "/api/stale",
      "module": "app.Api",
      "function": "handle",
      "arity": 1
    }
  ],
  "assets": [
    {
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    }
  ]
}
"#,
    )
    .expect("write stale handler manifest");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /api/stale HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream stale handler request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 502 "), "{response}");
    assert!(response.contains("content-type: text/plain; charset=utf-8\r\n"));
    assert!(
        response.contains("error[serve_runtime]: dynamic handler `app.Api.handle/1`"),
        "{response}"
    );
    assert!(response.contains("missing source metadata"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

#[test]
fn vm_stream_request_prefers_dynamic_handler_over_file_fallback_without_hyper() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_handler_before_file_fallback");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    fs::write(
        dir.join("terlan.toml"),
        "[package]\nname = \"serve_vm_stream_file_fallback_demo\"\nversion = \"0.0.7\"\n",
    )
    .expect("write project manifest");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    { "method": "GET", "route": "/api/game/config", "module": "app.Api", "function": "config", "arity": 1 }
  ],
  "file_responses": [
    { "method": "GET", "route": "*", "path": "index.html", "status": 200, "content_type": "text/html; charset=utf-8" }
  ],
  "assets": [
    {
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    }
  ]
}
"#,
    )
    .expect("write manifest");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /api/game/config HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream dynamic-before-file-fallback request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 502 "), "{response}");
    assert!(
        response.contains("error[serve_runtime]: dynamic handler `app.Api.config/1`"),
        "{response}"
    );
    assert!(!response.contains("<!doctype html>"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

#[test]
fn vm_stream_request_prefers_dynamic_handler_over_static_response_fallback_without_hyper() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_handler_before_static_response_fallback");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    fs::write(
        dir.join("terlan.toml"),
        "[package]\nname = \"serve_vm_stream_static_fallback_demo\"\nversion = \"0.0.7\"\n",
    )
    .expect("write project manifest");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    { "method": "GET", "route": "/api/game/config", "module": "app.Api", "function": "config", "arity": 1 }
  ],
  "static_responses": [
    { "method": "GET", "route": "*", "status": 404, "content_type": "text/plain; charset=utf-8", "body": "fallback" }
  ],
  "assets": [
    {
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    }
  ]
}
"#,
    )
    .expect("write manifest");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /api/game/config HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream dynamic-before-static-response-fallback request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 502 "), "{response}");
    assert!(
        response.contains("error[serve_runtime]: dynamic handler `app.Api.config/1`"),
        "{response}"
    );
    assert!(!response.ends_with("\r\n\r\nfallback"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

#[test]
fn vm_stream_request_serves_static_asset_without_hyper() {
    let dir = temp_dir("vm_stream_static_asset_success");
    let web_root = dir.join("web");
    write_valid_package(&web_root);

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /assets/hello.txt?cache=1 HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream static asset request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\nhello asset\n"));

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies VM-stream serving can read static assets emitted from `[web.assets]`.
///
/// Inputs:
/// - Project fixture with `[web.assets] directory = "assets"`.
/// - Browser build output produced by the real `terlc build` command.
///
/// Output:
/// - Test passes when the VM-owned HTTP path serves the copied manifest asset
///   by its public path without Hyper.
///
/// Transformation:
/// - Bridges the browser package build path and VM-stream serve path so
///   manifest asset copying and request routing are validated together.
#[test]
fn vm_stream_request_serves_manifest_declared_static_assets_from_build_output_without_hyper() {
    let dir = temp_dir("vm_stream_manifest_static_assets_from_build");
    let project_dir = dir.join("project");
    let source_dir = project_dir.join("src/demo");
    let asset_dir = project_dir.join("assets/nested");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(source_dir.join("assets")).expect("create source asset dir");
    fs::create_dir_all(&asset_dir).expect("create asset dir");
    fs::write(
        project_dir.join("terlan.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.7\"\n\n[build]\nsource_roots = [\"src\"]\n\n[web.assets]\ndirectory = \"assets\"\npublic_path = \"/assets\"\n",
    )
    .expect("write project manifest");
    fs::write(
        source_dir.join("Main.terl"),
        "module demo.Main.\n\nimport file \"./assets/browser.txt\" as BrowserAsset.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("write source");
    fs::write(source_dir.join("assets/browser.txt"), "browser asset\n")
        .expect("write source asset");
    fs::write(asset_dir.join("logo.txt"), "terlan asset\n").expect("write asset");

    let status = crate::commands::build::run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                project_dir.display().to_string(),
                "--target".to_string(),
                "js.browser".to_string(),
            ],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );
    assert_eq!(status, ExitCode::SUCCESS);

    let web_root = out_dir.join("web");
    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /assets/nested/logo.txt?cache=1 HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream generated static asset request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\nterlan asset\n"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_stream_request_prefers_physical_static_asset_over_static_response_fallback_without_hyper() {
    let dir = temp_dir("vm_stream_asset_before_static_response_fallback");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [],
  "static_responses": [
    { "method": "GET", "route": "*", "status": 404, "content_type": "text/plain; charset=utf-8", "body": "fallback" }
  ],
  "assets": [
    {
      "module": "app",
      "kind": "static-asset",
      "source_relative_path": "assets/hello.txt",
      "web_relative_path": "assets/hello.txt",
      "fingerprint": 1
    }
  ]
}
"#,
    )
    .expect("write manifest");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /assets/hello.txt HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream static asset precedence request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\nhello asset\n"));
    assert!(!response.contains("fallback"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_stream_request_prefers_physical_static_asset_over_file_fallback_without_hyper() {
    let dir = temp_dir("vm_stream_asset_before_file_fallback");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [],
  "file_responses": [
    { "method": "GET", "route": "*", "path": "index.html", "status": 200, "content_type": "text/html; charset=utf-8" }
  ],
  "assets": [
    {
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    }
  ]
}
"#,
    )
    .expect("write manifest");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /assets/js/modules/app.js HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream static asset before file fallback request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
    assert!(
        response.contains("content-type: text/javascript; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\nexport const value = 1;\n"));
    assert!(!response.contains("<!doctype html>"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_stream_request_serves_reload_sse_handshake_without_hyper() {
    let dir = temp_dir("vm_stream_reload_sse");
    let web_root = dir.join("web");
    fs::create_dir_all(&web_root).expect("create web root");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /__terlan/reload HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream reload SSE request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
    assert!(
        response.contains("content-type: text/event-stream\r\n"),
        "{response}"
    );
    assert!(
        response.contains("access-control-allow-origin: *\r\n"),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\n: connected\n\n"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies manifest-discovered SSE routes activate their source router graph.
#[test]
fn vm_stream_sse_route_activates_materialized_router_middleware() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_sse_router_middleware");
    let web_root = dir.join("_build/web");
    let source_dir = dir.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(&web_root).expect("create package dir");
    fs::write(
        dir.join("terlan.toml"),
        "[package]\nname = \"serve_sse_router\"\nversion = \"0.0.7\"\n",
    )
    .expect("write project manifest");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        source_dir.join("Events.terl"),
        r#"module app.Events.

import std.core.Option.{Some}.
import std.http.{Response, Router, Sse}.
import std.http.Router.{Continue, Respond}.
import type std.http.{Request, Response, Router}.
import type std.http.Router.MiddlewareResult.

pub gate(request: Request): MiddlewareResult ->
    case request.header("x-deny-stream") {
        Some(_) -> Respond(Response.text("stream denied", status = 401));
        _ -> Continue
    }.

pub decorate(_request: Request, response: Response): Response ->
    response.with_status(403).

pub router(): Router ->
    Router.use(Router.new(), gate)
        .map_response(decorate)
        .sse("/events", Sse.endpoint_with_keep_alive(8, 4096, 15000)).
"#,
    )
    .expect("write SSE router source");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "sse": [
    {
      "module": "app.Events",
      "route": "/events",
      "source": { "path": "src/app/Events.terl", "line": 18, "column": 5 }
    }
  ],
  "assets": []
}
"#,
    )
    .expect("write SSE manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm SSE router");

    let denied = handle_vm_stream_http1_request(
        &web_root,
        b"GET /events HTTP/1.1\r\nHost: localhost\r\nX-Deny-Stream: yes\r\n\r\n",
    )
    .expect("denied SSE request");
    let denied = String::from_utf8(denied).expect("denied response should be UTF-8");
    assert!(denied.starts_with("HTTP/1.1 403 "), "{denied}");
    assert!(denied.ends_with("\r\n\r\nstream denied"), "{denied}");

    let accepted = handle_vm_stream_http1_request(
        &web_root,
        b"GET /events HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("accepted SSE request");
    let accepted = String::from_utf8(accepted).expect("SSE response should be UTF-8");
    assert!(accepted.starts_with("HTTP/1.1 200 "), "{accepted}");
    assert!(
        accepted.contains("content-type: text/event-stream\r\n"),
        "{accepted}"
    );
    assert!(
        accepted.contains("cache-control: no-cache\r\n"),
        "{accepted}"
    );
    assert!(accepted.ends_with("\r\n\r\n: connected\n\n"), "{accepted}");

    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "sse": [
    {
      "module": "app.Events",
      "route": "/stale",
      "source": { "path": "src/app/Events.terl", "line": 18, "column": 5 }
    }
  ],
  "assets": []
}
"#,
    )
    .expect("write stale SSE manifest");
    let stale = handle_vm_stream_http1_request(
        &web_root,
        b"GET /stale HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("stale SSE request");
    let stale = String::from_utf8(stale).expect("stale response should be UTF-8");
    assert!(stale.starts_with("HTTP/1.1 502 "), "{stale}");
    assert!(
        stale.contains("error[serve_router]: materialized router did not match SSE GET /stale"),
        "{stale}"
    );
    assert!(!stale.contains("text/event-stream"), "{stale}");

    fs::remove_dir_all(dir).expect("cleanup");
}
