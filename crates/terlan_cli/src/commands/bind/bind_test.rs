use super::*;
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
/// - Starts from `crates/terlan_cli` and walks two parents to the repository
///   root used by committed std fixtures.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root")
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
fn parse_bind_rust_args_accepts_required_options() {
    let parsed = parse_bind_rust_args(&[
        "--crate".to_string(),
        "polars".to_string(),
        "--out".to_string(),
        "packages/std/native/polars".to_string(),
    ])
    .expect("bind rust args should parse");

    assert_eq!(parsed.crate_name, "polars");
    assert_eq!(parsed.out_dir, PathBuf::from("packages/std/native/polars"));
}

/// Verifies missing required options are rejected before generation.
///
/// Inputs:
/// - Synthetic `terlc bind rust` arguments without `--out`.
///
/// Output:
/// - Test assertion only.
///
/// Transformation:
/// - Parses command-local arguments and confirms the diagnostic remains
///   stable for roadmap gates.
#[test]
fn parse_bind_rust_args_requires_out_dir() {
    let err = parse_bind_rust_args(&["--crate".to_string(), "polars".to_string()])
        .expect_err("missing out dir should fail");

    assert_eq!(err, "terlc bind rust requires --out <dir>");
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

    assert!(out_dir.join("std/js/dom/document.terl").exists());
    assert!(out_dir.join("std/js/dom/document.terli").exists());
    assert!(out_dir.join("std/js/map.terl").exists());
    assert!(out_dir.join("std/js/set.terl").exists());
    assert!(out_dir
        .join("std/summaries/std.js.Dom.Document.typi")
        .exists());
    assert!(out_dir.join("std/summaries/std.js.Map.typi").exists());
    assert!(out_dir.join("std/js/dom/DocumentTest.terl").exists());
    assert!(out_dir
        .join("std/js/manifests/std_js_bindings.json")
        .exists());
    assert!(out_dir
        .join("std/js/manifests/std_js_skipped.json")
        .exists());

    let document_source =
        fs::read_to_string(out_dir.join("std/js/dom/document.terl")).expect("read source");
    assert!(document_source.contains("@generated true"));
    assert!(document_source.contains("@do-not-edit true"));
    assert!(document_source.contains("@generator terlc"));
    assert!(document_source.contains("@input-manifest std/js/manifests/std_js_dom_inputs.json"));
    assert!(document_source.contains(
        "@source-input std/js/fixtures/lib.es5.d.ts sha256=c430d44666289dae81f30fa7b2edebf186ecc91a2d4c71266ea6ae76388792e1"
    ));
    assert!(document_source.contains(
        "@source-input std/js/fixtures/lib.es2015.collection.d.ts sha256=dc2df20b1bcdc8c2d34af4926e2c3ab15ffe1160a63e58b7e09833f616efff44"
    ));
    assert!(document_source.contains(
        "@source-input std/js/fixtures/lib.dom.d.ts sha256=080941d9f9ff9307f7e27a83bcd888b7c8270716c39af943532438932ec1d0b9"
    ));
    assert!(document_source.contains("module std.js.Dom.Document."));
    assert!(document_source.contains("pub opaque type Document."));
    assert!(document_source.contains(
        "pub (value: Document) get_element_by_id(element_id: std.js.String.JsString): Option[HTMLElement] ->"
    ));
    assert!(document_source.contains("    native."));

    let document_test = fs::read_to_string(out_dir.join("std/js/dom/DocumentTest.terl"))
        .expect("read generated DOM test");
    assert!(document_test.contains("@artifact-kind test"));
    assert!(document_test.contains("module std.js.Dom.DocumentTest."));
    assert!(document_test.contains("import type std.js.Dom.Document.Document."));
    assert!(document_test.contains("@test\npub generated_binding_surface_exists(): Bool ->"));
    assert!(document_test.contains(
        "pub get_element_by_id_typechecks(receiver: Document, element_id: std.js.String.JsString): Option[HTMLElement] ->"
    ));
    assert!(document_test.contains("    receiver.get_element_by_id(element_id)."));

    let map_source =
        fs::read_to_string(out_dir.join("std/js/map.terl")).expect("read generated map source");
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
    assert!(binding_manifest.contains("\"test\": \"std/js/dom/DocumentTest.terl\""));
    assert!(
        binding_manifest.contains("\"skipped_manifest\": \"std/js/manifests/std_js_skipped.json\"")
    );

    let skipped_manifest = fs::read_to_string(out_dir.join("std/js/manifests/std_js_skipped.json"))
        .expect("read skipped manifest");
    assert!(skipped_manifest.contains("\"schema\": \"terlan.std.js.skipped-declarations.v1\""));
    assert!(skipped_manifest.contains("\"source\": \"Map.set\""));
    assert!(skipped_manifest.contains("\"source\": \"std.js.NaN\""));
    assert!(skipped_manifest.contains("\"reason\": \"ts_bindgen.unsupported_type\""));

    fs::remove_dir_all(out_dir).expect("remove generated bindings");
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
    assert!(native_abi.contains("supervision = \"std.beam.NativeBridge.NativeBridgeRuntime\""));
    assert!(native_abi.contains("process = \"std.beam.Process.Process\""));
    assert!(native_abi.contains("message = \"std.beam.Message.MessageCodec\""));
    assert!(native_abi.contains("backpressure = \"std.beam.Backpressure.Backpressure\""));
    assert!(native_abi.contains("credit = \"std.beam.Backpressure.Credit\""));
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
