use super::*;

/// Verifies lowercase TypeScript legacy globals become valid Terlan type names.
#[test]
fn js_dom_type_name_normalizes_lowercase_legacy_global() {
    assert_eq!(
        "WebkitURL",
        ts_dom_generator::render_type_name("webkitURL", &[])
    );
    assert_eq!(
        "WebkitURL[T]",
        ts_dom_generator::render_type_name("webkitURL", &["T".to_string()])
    );
}
use crate::support::test_fs;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

/// Creates a unique temporary directory for bind command tests.
///
/// Inputs:
/// - `name`: stable test label included in the directory name.
///
/// Output:
/// - A directory path that does not exist yet.
///
/// Transformation:
/// - Combines the process id and current timestamp so parallel test runs do
///   not share generated package output.
fn temp_output_dir(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    path.push(format!("terlan_bind_{name}_{}_{}", std::process::id(), now));
    path
}

/// Returns the repository root for bind-command integration tests.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Absolute repository root path.
///
/// Transformation:
/// - Starts from `crates/terlan` and walks two parents to the repository
///   root used by committed std fixtures.
fn repo_root() -> PathBuf {
    test_fs::repo_root()
}

/// Computes lowercase SHA-256 for temporary manifest fixtures.
fn test_sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Verifies valid Rust binding arguments parse into the reserved shape.
///
/// Inputs:
/// - Synthetic `--crate` and `--out` arguments.
///
/// Output:
/// - Test assertions only.
///
/// Transformation:
/// - Parses command-local arguments and confirms the crate name and output
///   directory are preserved exactly.
#[test]
fn parse_bind_native_args_accepts_required_options() {
    let parsed = parse_bind_native_args(&[
        "--crate".to_string(),
        "polars".to_string(),
        "--out".to_string(),
        "packages/std/native/polars".to_string(),
    ])
    .expect("bind native args should parse");

    assert_eq!(parsed.crate_name, "polars");
    assert_eq!(parsed.out_dir, PathBuf::from("packages/std/native/polars"));
}

/// Verifies missing required options are rejected before generation.
///
/// Inputs:
/// - Synthetic `terlc bind native` arguments without `--out`.
///
/// Output:
/// - Test assertion only.
///
/// Transformation:
/// - Parses command-local arguments and confirms the diagnostic remains
///   stable for roadmap gates.
#[test]
fn parse_bind_native_args_requires_out_dir() {
    let err = parse_bind_native_args(&["--crate".to_string(), "polars".to_string()])
        .expect_err("missing out dir should fail");

    assert_eq!(err, "terlc bind native requires --out <dir>");
}

/// Verifies valid JS DOM binding arguments parse into the reserved shape.
///
/// Inputs:
/// - Synthetic `--manifest` and `--out` arguments.
///
/// Output:
/// - Test assertions only.
///
/// Transformation:
/// - Parses command-local arguments and confirms the manifest and output paths
///   are preserved exactly.
#[test]
fn parse_bind_js_dom_args_accepts_required_options() {
    let parsed = parse_bind_js_dom_args(&[
        "--manifest".to_string(),
        "std/js/manifests/std_js_dom_inputs.json".to_string(),
        "--out".to_string(),
        "generated-js-dom".to_string(),
    ])
    .expect("bind js-dom args should parse");

    assert_eq!(
        parsed.manifest_path,
        PathBuf::from("std/js/manifests/std_js_dom_inputs.json")
    );
    assert_eq!(parsed.out_dir, PathBuf::from("generated-js-dom"));
}

/// Verifies missing JS DOM output options are rejected before generation.
///
/// Inputs:
/// - Synthetic `terlc bind js-dom` arguments without `--out`.
///
/// Output:
/// - Test assertion only.
///
/// Transformation:
/// - Parses command-local arguments and confirms the diagnostic remains stable
///   for the first generated DOM binding command.
#[test]
fn parse_bind_js_dom_args_requires_out_dir() {
    let err = parse_bind_js_dom_args(&[
        "--manifest".to_string(),
        "std/js/manifests/std_js_dom_inputs.json".to_string(),
    ])
    .expect_err("missing out dir should fail");

    assert_eq!(err, "terlc bind js-dom requires --out <dir>");
}

/// Verifies the compiler-managed Angular.ts binding command accepts only its
/// deterministic output location.
#[test]
fn parse_bind_angular_ts_args_accepts_required_output() {
    let parsed =
        parse_bind_angular_ts_args(&["--out".to_string(), "generated-angular-ts".to_string()])
            .expect("bind angular-ts args should parse");

    assert_eq!(parsed.out_dir, PathBuf::from("generated-angular-ts"));
}

/// Verifies the Angular.ts binding command cannot silently choose an output
/// directory on behalf of a caller.
#[test]
fn parse_bind_angular_ts_args_requires_out_dir() {
    let err = parse_bind_angular_ts_args(&[]).expect_err("missing out dir should fail");

    assert_eq!(err, "terlc bind angular-ts requires --out <dir>");
}

/// Verifies valid native binding arguments parse into the manifest-backed shape.
///
/// Inputs:
/// - Synthetic `--manifest` and `--out` arguments.
///
/// Output:
/// - Test assertions only.
///
/// Transformation:
/// - Locks the public `terlc bind cpp` command shape before expanding
///   native package generation beyond fixture manifests.
#[test]
fn parse_bind_cpp_args_accepts_required_options() {
    let parsed = parse_bind_cpp_args(&[
        "--manifest".to_string(),
        "native/html/native-binding.json".to_string(),
        "--out".to_string(),
        "packages/std/native/html".to_string(),
    ])
    .expect("bind cpp args should parse");

    assert_eq!(
        parsed.manifest_path,
        PathBuf::from("native/html/native-binding.json")
    );
    assert_eq!(parsed.out_dir, PathBuf::from("packages/std/native/html"));
}

/// Verifies valid C ABI binding arguments parse into the manifest-backed shape.
#[test]
fn parse_bind_c_args_accepts_required_options() {
    let parsed = parse_bind_c_args(&[
        "--manifest".to_string(),
        "native/pjrt/native-binding.json".to_string(),
        "--out".to_string(),
        "packages/pjrt".to_string(),
    ])
    .expect("bind c args should parse");

    assert_eq!(
        parsed.manifest_path,
        PathBuf::from("native/pjrt/native-binding.json")
    );
    assert_eq!(parsed.out_dir, PathBuf::from("packages/pjrt"));
}

/// Verifies the JS DOM generator writes deterministic fixture outputs.
///
/// Inputs:
/// - Repository root fixture manifest and a temporary output directory.
///
/// Output:
/// - Test assertions over generated source, interface, summary, and manifest
///   files.
///
/// Transformation:
/// - Runs the public generator function against the committed TypeScript
///   standard-library fixtures without npm resolution or network access.
#[test]
fn generate_js_dom_bindings_writes_fixture_outputs() {
    let out_dir = temp_output_dir("js_dom_bindings");
    let repo_root = repo_root();

    generate_js_dom_bindings(
        &repo_root,
        Path::new("std/js/manifests/std_js_dom_inputs.json"),
        &out_dir,
    )
    .expect("JS DOM generation should succeed");

    assert!(out_dir.join("std/js/dom/Document.terl").exists());
    assert!(out_dir.join("std/js/dom/Document.terli").exists());
    assert!(out_dir.join("std/js/Map.terl").exists());
    assert!(out_dir.join("std/js/Set.terl").exists());
    assert!(out_dir
        .join("std/summaries/std.js.Dom.Document.typi")
        .exists());
    assert!(out_dir.join("std/summaries/std.js.Map.typi").exists());
    assert!(out_dir.join("std/js/dom/DocumentTest.terl").exists());
    assert!(out_dir.join("std/js/dom/WebkitURL.terl").exists());
    assert!(out_dir.join("std/js/dom/WebkitURL.terli").exists());
    assert!(out_dir.join("std/js/dom/WebkitURLTest.terl").exists());
    assert!(out_dir
        .join("std/summaries/std.js.Dom.WebkitURL.typi")
        .exists());
    assert!(!out_dir.join("std/js/dom/webkitURL.terl").exists());
    assert!(!out_dir.join("std/js/dom/webkitURL.terli").exists());
    assert!(!out_dir.join("std/js/dom/webkitURLTest.terl").exists());
    assert!(!out_dir
        .join("std/summaries/std.js.Dom.webkitURL.typi")
        .exists());
    assert!(out_dir
        .join("std/js/manifests/std_js_bindings.json")
        .exists());
    assert!(out_dir
        .join("std/js/manifests/std_js_skipped.json")
        .exists());

    let document_source =
        fs::read_to_string(out_dir.join("std/js/dom/Document.terl")).expect("read source");
    assert!(document_source.contains("@generated true"));
    assert!(document_source.contains("@do-not-edit true"));
    assert!(document_source.contains("@generator terlc"));
    assert!(document_source.contains("@generator-version 0.0.7"));
    assert!(document_source.contains("@generator-profile typescript-standard-js-dom"));
    assert!(document_source.contains("@artifact-kind source"));
    assert!(document_source.contains("@input-manifest std/js/manifests/std_js_dom_inputs.json"));
    assert!(document_source.contains("@source-package typescript@5.9.3"));
    assert!(document_source.contains("@source-input std/js/fixtures/lib.es5.d.ts"));
    assert!(document_source.contains("@source-interface Document"));
    assert!(document_source.contains("module std.js.Dom.Document."));
    assert!(document_source.contains("pub opaque type Document."));
    assert!(document_source.contains(
        "pub (value: Document) get_element_by_id(element_id: JsString): Option[HTMLElement] ->"
    ));
    assert!(document_source.contains("    native."));
    assert_eq!(
        crate::terlan_syntax::format_source_module(&document_source)
            .expect("format generated DOM source"),
        document_source
    );

    let webkit_url_source =
        fs::read_to_string(out_dir.join("std/js/dom/WebkitURL.terl")).expect("read source");
    assert!(webkit_url_source.contains("@source-interface webkitURL"));
    assert!(webkit_url_source.contains("module std.js.Dom.WebkitURL."));
    assert!(webkit_url_source.contains("pub type WebkitURL = URL."));

    let document_test = fs::read_to_string(out_dir.join("std/js/dom/DocumentTest.terl"))
        .expect("read generated DOM test");
    assert!(document_test.contains("@generated true"));
    assert!(document_test.contains("@do-not-edit true"));
    assert!(document_test.contains("@generator terlc"));
    assert!(document_test.contains("@generator-version 0.0.7"));
    assert!(document_test.contains("@generator-profile typescript-standard-js-dom"));
    assert!(document_test.contains("@artifact-kind test"));
    assert!(document_test.contains("@input-manifest std/js/manifests/std_js_dom_inputs.json"));
    assert!(document_test.contains("@source-package typescript@5.9.3"));
    assert!(document_test.contains("@source-input std/js/fixtures/lib.es5.d.ts"));
    assert!(document_test.contains("@source-interface Document"));
    assert!(document_test.contains("module std.js.Dom.DocumentTest."));
    assert!(document_test.contains("import type std.js.Dom.Document."));
    assert!(document_test.contains("pub generated_binding_surface_contract(): Bool ->"));
    assert!(!document_test.contains("@test\npub generated_binding_surface_contract(): Bool ->"));
    assert!(document_test.contains(
        "pub get_element_by_id_typechecks(receiver: Document, element_id: JsString): Option[HTMLElement] ->"
    ));
    assert!(document_test.contains("    receiver.get_element_by_id(element_id)."));

    let map_source =
        fs::read_to_string(out_dir.join("std/js/Map.terl")).expect("read generated map source");
    assert!(map_source.contains("module std.js.Map."));
    assert!(map_source.contains("pub opaque type Map[K, V]."));
    assert!(
        map_source.contains("@returns true if an element in the Map existed and has been removed")
    );
    assert!(map_source.contains("pub (value: Map[K, V]) get(key: K): Option[V] ->"));
    assert!(map_source.contains("Returns a specified element from the Map object."));
    assert!(map_source.contains("pub (value: Map[K, V]) size(): std.js.Number.JsNumber ->"));
    assert!(map_source.contains("@returns the number of elements in the Map."));

    let binding_manifest =
        fs::read_to_string(out_dir.join("std/js/manifests/std_js_bindings.json"))
            .expect("read binding manifest");
    assert!(binding_manifest.contains("\"schema\": \"terlan.std.js.bindings.v1\""));
    assert!(binding_manifest.contains("\"module\": \"std.js.Dom.Document\""));
    assert!(binding_manifest.contains("\"module\": \"std.js.Map\""));
    assert!(binding_manifest.contains("\"summary\": \"std/summaries/std.js.Dom.Document.typi\""));
    assert!(binding_manifest.contains("\"summary\": \"std/summaries/std.js.Map.typi\""));
    assert!(binding_manifest.contains("\"source\": \"std/js/dom/Document.terl\""));
    assert!(binding_manifest.contains("\"interface\": \"std/js/dom/Document.terli\""));
    assert!(binding_manifest.contains("\"test\": \"std/js/dom/DocumentTest.terl\""));
    assert!(
        binding_manifest.contains("\"skipped_manifest\": \"std/js/manifests/std_js_skipped.json\"")
    );
    let binding_json: serde_json::Value =
        serde_json::from_str(&binding_manifest).expect("parse binding manifest");
    let generated_files = binding_json["generated_files"]
        .as_array()
        .expect("generated file hash entries");
    let mut generated_file_paths = BTreeSet::new();
    for entry in generated_files {
        let path = entry["path"].as_str().expect("generated file path");
        let sha256 = entry["sha256"].as_str().expect("generated file hash");
        assert!(
            generated_file_paths.insert(path.to_string()),
            "duplicate generated file hash path {path}"
        );
        assert_eq!(sha256.len(), 64, "generated file hash length for {path}");
        assert!(
            sha256
                .chars()
                .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()),
            "generated file hash must be lowercase hex for {path}"
        );
    }
    assert!(generated_file_paths.contains("std/js/dom/Document.terl"));
    assert!(generated_file_paths.contains("std/js/dom/Document.terli"));
    assert!(generated_file_paths.contains("std/summaries/std.js.Dom.Document.typi"));
    assert!(generated_file_paths.contains("std/js/dom/DocumentTest.terl"));
    assert!(!generated_file_paths.contains("std/js/manifests/std_js_bindings.json"));

    let skipped_manifest = fs::read_to_string(out_dir.join("std/js/manifests/std_js_skipped.json"))
        .expect("read skipped manifest");
    assert!(skipped_manifest.contains("\"schema\": \"terlan.std.js.skipped-declarations.v1\""));
    assert!(skipped_manifest.contains("\"source\": \"Map.set\""));
    assert!(skipped_manifest.contains("\"source\": \"std.js.NaN\""));
    assert!(skipped_manifest.contains("\"reason\": \"ts_bindgen.unsupported_type\""));
    let skipped_json: serde_json::Value =
        serde_json::from_str(&skipped_manifest).expect("parse skipped manifest");
    assert_eq!(
        skipped_json["schema"].as_str(),
        Some("terlan.std.js.skipped-declarations.v1")
    );
    let skipped_entries = skipped_json["skipped"]
        .as_array()
        .expect("skipped manifest entries");
    let mut skipped_sources = BTreeSet::new();
    for entry in skipped_entries {
        let source = entry["source"].as_str().expect("skipped source");
        let reason = entry["reason"].as_str().expect("skipped reason");
        let detail = entry["detail"].as_str().expect("skipped detail");
        assert!(
            skipped_sources.insert(source.to_string()),
            "duplicate skipped source {source}"
        );
        assert!(
            reason.starts_with("ts_bindgen."),
            "unstable skipped reason {reason}"
        );
        assert!(!detail.is_empty(), "empty skipped detail for {source}");
    }

    fs::remove_dir_all(out_dir).expect("remove generated bindings");
}

/// Verifies Angular.ts namespace generation emits a callable Terlan facade.
///
/// Inputs:
/// - Temporary Angular-style `@types/namespace.d.ts` manifest.
///
/// Output:
/// - Test assertions over generated `terlan.angular.ng.*` aliases and the
///   generated `terlan.angular.Ng` callable facade.
///
/// Transformation:
/// - Pins the release requirement that Angular.ts namespace generation is not
///   declaration-only; app authors need Terlan entry points for module,
///   component, directive, scope, template-cache, and HTTP workflows.
#[test]
fn generate_js_dom_bindings_writes_angular_namespace_facade() {
    let root = temp_output_dir("angular_namespace_root");
    let out_dir = temp_output_dir("angular_namespace_out");
    let namespace_path = root.join("@types/namespace.d.ts");
    fs::create_dir_all(namespace_path.parent().expect("namespace parent"))
        .expect("create namespace parent");
    let namespace_source = r#"declare global {
  export namespace ng {
    type Angular = TAngular;
    type NgModule = TNgModule;
    type Component = { template: string };
    type Directive<TController = unknown> = TDirective<TController>;
    type Scope = TScope;
    type TemplateCacheService = Map<string, string>;
    type HttpService = THttpService;
    type HttpResponse<T> = THttpResponse<T>;
    type Machine<T> = TMachine<T>;
    type MachineConfig<T> = TMachineConfig<T>;
    type MachineSendResult<T> = TMachineSendResult<T>;
    type MachineService = TMachineService;
    type MachineSnapshot<T> = TMachineSnapshot<T>;
    type Workflow<T> = TWorkflow<T>;
    type WorkflowConfig<T> = TWorkflowConfig<T>;
    type WorkflowResult<T> = TWorkflowResult<T>;
    type WorkflowService = TWorkflowService;
    type WorkflowSnapshot<T> = TWorkflowSnapshot<T>;
    type SseConfig = TSseConfig;
    type SseConnection = TSseConnection;
    type SseService = TSseService;
    type WebSocketConfig = TWebSocketConfig;
    type WebSocketConnection = TWebSocketConnection;
    type WebSocketService = TWebSocketService;
    type WorkerConfig<T> = TWorkerConfig<T>;
    type WorkerHandle<TSend, TReceive> = TWorkerHandle<TSend, TReceive>;
    type WorkerService = TWorkerService;
  }
}
"#;
    fs::write(&namespace_path, namespace_source).expect("write namespace fixture");
    let namespace_hash = test_sha256_hex(namespace_source.as_bytes());
    let manifest_path = root.join("manifest.json");
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "schema": "terlan.std.js.input-manifest.v1",
  "generator": {{
    "name": "terlc",
    "version": "0.0.7",
    "profile": "angular-ts-namespace",
    "oxc_parser": true
  }},
  "target_profile": "js.browser",
  "source_package": {{
    "name": "typescript",
    "version": "local",
    "resolution": "@types/namespace.d.ts"
  }},
  "inputs": [
    {{
      "path": "@types/namespace.d.ts",
      "sha256": "{namespace_hash}",
      "kind": "typescript-declaration",
      "namespace": "terlan.angular"
    }}
  ]
}}
"#
        ),
    )
    .expect("write namespace manifest");

    generate_js_dom_bindings(&root, &manifest_path, &out_dir)
        .expect("Angular namespace generation should succeed");

    let facade_source =
        fs::read_to_string(out_dir.join("terlan/angular/Ng.terl")).expect("read facade source");
    assert!(facade_source.contains("module terlan.angular.Ng."));
    assert!(facade_source.contains("import type std.js.String.{JsString}."));
    assert!(facade_source.contains("import type terlan.angular.ng.{"));
    assert!(facade_source.contains("NgModule"));
    assert!(facade_source.contains("TemplateCacheService"));
    assert!(facade_source.contains("pub angular(): terlan.angular.ng.Angular.Angular ->"));
    assert!(facade_source.contains("pub ng_module(name: JsString): NgModule ->"));
    assert!(facade_source.contains("pub register_component("));
    assert!(
        facade_source.contains("pub apply_scope(scope: terlan.angular.ng.Scope.Scope): Unit ->")
    );
    assert!(facade_source.contains("pub template_put("));
    assert!(facade_source.contains("pub http_get("));
    for marker in [
        "pub machine(",
        "pub workflow(",
        "pub sse_connect(service:",
        "pub websocket_connect(service:",
        "pub worker_start(service:",
        "pub worker_on_message(worker:",
        "pub directive_with_link(",
        "pub template_get(templates:",
    ] {
        assert!(
            facade_source.contains(marker),
            "missing facade marker {marker}"
        );
    }
    assert!(!facade_source.contains("pub opaque type Ng"));
    assert_eq!(
        crate::terlan_syntax::format_source_module(&facade_source)
            .expect("format generated Angular facade"),
        facade_source
    );

    let manifest = fs::read_to_string(out_dir.join("std/js/manifests/std_js_bindings.json"))
        .expect("read binding manifest");
    assert!(manifest.contains("\"module\": \"terlan.angular.Ng\""));
    assert!(manifest.contains("\"source\": \"terlan/angular/Ng.terl\""));
    assert!(manifest.contains("\"summary\": \"std/summaries/terlan.angular.Ng.typi\""));
    assert!(manifest.contains("\"test\": \"terlan/angular/NgTest.terl\""));

    fs::remove_dir_all(root).expect("remove namespace root");
    fs::remove_dir_all(out_dir).expect("remove generated namespace bindings");
}

/// Verifies JS DOM generation refuses existing non-empty directories.
///
/// Inputs:
/// - Temporary directory containing one placeholder file.
///
/// Output:
/// - Test assertion over the refusal diagnostic.
///
/// Transformation:
/// - Confirms binding generation stops before writing fixture outputs into an
///   unsafe destination.
#[test]
fn generate_js_dom_bindings_refuses_non_empty_output_directory() {
    let out_dir = temp_output_dir("js_dom_non_empty");
    fs::create_dir_all(&out_dir).expect("create output dir");
    fs::write(out_dir.join("existing.txt"), "existing").expect("write existing file");

    let err = generate_js_dom_bindings(
        &repo_root(),
        Path::new("std/js/manifests/std_js_dom_inputs.json"),
        &out_dir,
    )
    .expect_err("non-empty output directory should fail");

    assert!(err.contains("refusing to generate into non-empty output directory"));

    fs::remove_dir_all(out_dir).expect("remove generated bindings");
}

/// Verifies the Polars generator writes the curated skeleton.
///
/// Inputs:
/// - Temporary output directory that does not exist before generation.
///
/// Output:
/// - Test assertions over generated files.
///
/// Transformation:
/// - Runs the deterministic package writer and confirms the package
///   manifest, Terlan source, interface summary, mapping metadata, package
///   docs, example source, native ABI metadata, Rust manifest, and Rust
///   stub exist with the current native error-conversion contract.
#[test]
fn generate_package_writes_polars_skeleton() {
    let out_dir = temp_output_dir("polars_skeleton");

    generate_package(&out_dir, POLARS_FILES).expect("package generation should succeed");

    assert!(out_dir.join("terlan.toml").exists());
    assert!(out_dir
        .join("src/std/native/polars/DataFrame.terl")
        .exists());
    assert!(out_dir.join("bindings/polars.mapping.toml").exists());
    assert!(out_dir.join("native/terlan-native.toml").exists());
    assert!(out_dir.join("docs/std.native.polars.md").exists());
    assert!(out_dir.join("examples/read_csv.terl").exists());
    assert!(out_dir
        .join("summaries/std.native.polars.DataFrame.typi")
        .exists());
    assert!(out_dir.join("native/rust/Cargo.toml").exists());
    assert!(out_dir.join("native/rust/src/lib.rs").exists());
    assert!(out_dir.join("native/rust/src/bridge.rs").exists());

    let dataframe_source = fs::read_to_string(out_dir.join("src/std/native/polars/DataFrame.terl"))
        .expect("read generated DataFrame source");
    assert!(dataframe_source.contains("@example target rust"));

    let mapping = fs::read_to_string(out_dir.join("bindings/polars.mapping.toml"))
        .expect("read generated Polars mapping");
    assert!(mapping.contains("conversion = \"code_message\""));

    let native_abi = fs::read_to_string(out_dir.join("native/terlan-native.toml"))
        .expect("read generated native ABI metadata");
    assert!(native_abi.contains("bridge = \"supervised_actor\""));
    assert!(native_abi.contains("worker = \"rust_thread_probe\""));
    assert!(native_abi.contains("ownership = \"opaque_handles\""));
    assert!(native_abi.contains("backpressure = \"credit\""));
    assert!(native_abi.contains("handle_generation_tokens = true"));
    assert!(native_abi.contains("[runtime.commands]"));
    assert!(native_abi.contains("[runtime.beam]"));
    assert!(native_abi.contains("supervision = \"std.vm.NativeBridge.NativeBridgeRuntime\""));
    assert!(native_abi.contains("process = \"std.vm.Process.Process\""));
    assert!(native_abi.contains("message = \"std.vm.Message.MessageCodec\""));
    assert!(native_abi.contains("backpressure = \"std.vm.Backpressure.Backpressure\""));
    assert!(native_abi.contains("credit = \"std.vm.Backpressure.Credit\""));
    assert!(native_abi.contains("native_unavailable_code = \"native_unavailable\""));
    assert!(native_abi.contains("[result_conversions.\"std.native.polars.DataFrame.read_csv\"]"));
    assert!(native_abi.contains("[result_conversions.\"std.native.polars.DataFrame.select\"]"));

    let rust_adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read generated Rust adapter");
    assert!(rust_adapter.contains("#![forbid(unsafe_code)]"));
    assert!(rust_adapter.contains("fn adapter_error_converts_to_code_message_parts()"));
    assert!(rust_adapter.contains("pub mod bridge;"));

    let rust_bridge = fs::read_to_string(out_dir.join("native/rust/src/bridge.rs"))
        .expect("read generated Rust bridge");
    assert!(rust_bridge.contains("#![forbid(unsafe_code)]"));
    assert!(rust_bridge.contains("SupervisedNativeWorker"));
    assert!(rust_bridge.contains("stale_native_handle"));
    assert!(rust_bridge.contains("NativeBoundary worker"));

    fs::remove_dir_all(out_dir).expect("remove generated package");
}

/// Verifies generation refuses existing non-empty directories.
///
/// Inputs:
/// - Temporary directory containing one placeholder file.
///
/// Output:
/// - Test assertion over the refusal diagnostic.
///
/// Transformation:
/// - Creates a non-empty output path and confirms generation stops before
///   writing template files.
#[test]
fn generate_package_refuses_non_empty_output_directory() {
    let out_dir = temp_output_dir("non_empty");
    fs::create_dir_all(&out_dir).expect("create output dir");
    fs::write(out_dir.join("existing.txt"), "existing").expect("write existing file");

    let err = generate_package(&out_dir, POLARS_FILES)
        .expect_err("non-empty output directory should fail");

    assert!(err.contains("refusing to generate into non-empty output directory"));

    fs::remove_dir_all(out_dir).expect("remove generated package");
}
