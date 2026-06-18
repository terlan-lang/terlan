use super::*;

/// Asserts a JS runtime smoke status accepted by the J0.4 contract.
///
/// Inputs:
/// - `value`: manifest JSON value stored on one JS module artifact entry.
///
/// Output:
/// - Test assertion only; panics when the status is not a known runtime-smoke
///   result.
///
/// Transformation:
/// - Accepts successful runtime smoke when Node is available and explicit skip
///   status when the local runtime is unavailable.
fn assert_runtime_smoke_status(value: &serde_json::Value) {
    let status = value.as_str().expect("runtime smoke status");
    assert!(
        status == "passed" || status == "skipped:node_unavailable",
        "unexpected runtime smoke status: {status}"
    );
}

/// Verifies single-file builds emit Erlang source and BEAM bytecode only.
///
/// Inputs:
/// - A standalone Terlan source file with one public function.
/// - An explicit Erlang build target and isolated output directory.
///
/// Output:
/// - Test passes when Erlang source, BEAM bytecode, and a debug map are written
///   without package launcher artifacts.
///
/// Transformation:
/// - Runs the build command in single-file mode and checks that the compiler
///   records the source module, target, source path, and nonzero CoreIR hash in
///   the debug map.
#[test]
fn build_command_emits_erlang_source_and_beam_for_single_file() {
    let dir = make_temp_dir("single_file");
    let source_path = dir.join("build_single_file.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module build_single_file.\n\npub add(x: Int, y: Int): Int ->\n    x + y.\n",
    )
    .expect("failed to write source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_path.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(out_dir.join("src/build_single_file.erl").exists());
    assert!(out_dir.join("ebin/build_single_file.beam").exists());
    assert!(
        !out_dir.join("bin/build_single_file").exists(),
        "single-file builds should not emit a package executable launcher"
    );
    assert!(
        !out_dir.join(BUILD_PACKAGE_METADATA_FILE).exists(),
        "single-file builds should not emit package metadata"
    );

    let debug_map_text =
        fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE)).expect("read build debug map");
    let debug_map: serde_json::Value =
        serde_json::from_str(&debug_map_text).expect("parse build debug map");
    assert_eq!(debug_map["schema"], BUILD_DEBUG_MAP_SCHEMA);
    assert_eq!(debug_map["target"], "erlang");
    assert_eq!(debug_map["modules"].as_array().expect("modules").len(), 1);
    assert_eq!(debug_map["modules"][0]["module"], "build_single_file");
    assert_eq!(
        debug_map["modules"][0]["source_path"],
        source_path.to_string_lossy().to_string()
    );
    assert!(
        debug_map["modules"][0]["core_ir_hash"]
            .as_u64()
            .expect("core hash")
            > 0
    );
    assert_eq!(
        debug_map["modules"][0]["erl_path"],
        out_dir
            .join("src/build_single_file.erl")
            .to_string_lossy()
            .to_string()
    );
    assert_eq!(
        debug_map["modules"][0]["beam_path"],
        out_dir
            .join("ebin/build_single_file.beam")
            .to_string_lossy()
            .to_string()
    );
}

/// Verifies single-file JavaScript builds emit JS modules and a manifest.
///
/// Inputs:
/// - A standalone Terlan source file with one public arithmetic function.
/// - An explicit JavaScript build target and isolated output directory.
///
/// Output:
/// - Test passes when a `.js` module, target metadata, diagnostics metadata,
///   and JS build manifest are written without Erlang artifacts.
///
/// Transformation:
/// - Runs the build command through `--target js`, then inspects the J0.1
///   `_build/js`-style layout under the selected test output directory.
#[test]
fn build_command_emits_js_module_and_manifest_for_single_file() {
    let dir = make_temp_dir("single_file_js");
    let source_path = dir.join("build_single_file_js.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module build_single_file_js.\n\npub add(x: Int, y: Int): Int ->\n    x + y.\n",
    )
    .expect("failed to write source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_path.display().to_string(),
            "--target".to_string(),
            "js".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_root = out_dir.join("js");
    let js_module = js_root.join("modules/build_single_file_js.js");
    assert!(js_module.exists(), "expected JS module at {js_module:?}");
    assert!(
        !out_dir.join("src/build_single_file_js.erl").exists(),
        "JS build should not emit Erlang source"
    );
    assert!(
        !out_dir.join("ebin/build_single_file_js.beam").exists(),
        "JS build should not emit BEAM bytecode"
    );

    let js_text = fs::read_to_string(&js_module).expect("read JS module");
    assert!(js_text.contains("export function add(x, y)"));
    assert!(js_text.contains("return x + y;"));

    let manifest_text =
        fs::read_to_string(js_root.join("manifest.json")).expect("read JS manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse JS manifest");
    assert_eq!(manifest["schema"], "terlan-js-build-v1");
    assert_eq!(manifest["target_profile"], "js.shared");
    assert_eq!(manifest["module_format"], "es-module");
    assert_eq!(manifest["module_extension"], "js");
    assert_eq!(manifest["modules"].as_array().expect("modules").len(), 1);
    assert_eq!(manifest["modules"][0]["module"], "build_single_file_js");
    assert_eq!(
        manifest["modules"][0]["relative_path"],
        "modules/build_single_file_js.js"
    );
    assert_runtime_smoke_status(&manifest["modules"][0]["runtime_smoke_status"]);

    let profile_text = fs::read_to_string(js_root.join("metadata/target-profile.json"))
        .expect("read JS target metadata");
    let profile: serde_json::Value =
        serde_json::from_str(&profile_text).expect("parse JS target metadata");
    assert_eq!(profile["target_profile"], "js.shared");

    let diagnostics_text = fs::read_to_string(js_root.join("metadata/diagnostics.json"))
        .expect("read JS diagnostics metadata");
    let diagnostics: serde_json::Value =
        serde_json::from_str(&diagnostics_text).expect("parse JS diagnostics metadata");
    assert_eq!(diagnostics["diagnostic_family"], "js_emit");
    assert_eq!(
        diagnostics["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .len(),
        0
    );
}

/// Verifies JavaScript builds lower selected portable `std.core.String`
/// intrinsics directly into JS operations.
///
/// Inputs:
/// - A standalone Terlan source file using `String` receiver methods selected
///   for J0.7.
/// - An explicit JavaScript build target and isolated output directory.
///
/// Output:
/// - Test passes when the written JS module contains direct JavaScript string
///   operations and the manifest records the artifact.
///
/// Transformation:
/// - Runs the real `terlc build --target js` command so target validation,
///   direct Oxc emission, artifact writing, and manifest generation are all
///   exercised through the release-facing build path.
#[test]
fn build_command_emits_js_std_core_string_intrinsics() {
    let dir = make_temp_dir("single_file_js_string_intrinsics");
    let source_path = dir.join("build_single_file_js_string_intrinsics.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "\
module build_single_file_js_string_intrinsics.

pub clean(): String ->
    \"  hello  \".trim().

pub loud(): String ->
    \"hello\".uppercase().

pub has_suffix(): Bool ->
    \"hello\".ends_with(\"lo\").
",
    )
    .expect("failed to write source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_path.display().to_string(),
            "--target".to_string(),
            "js".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_root = out_dir.join("js");
    let js_module = js_root.join("modules/build_single_file_js_string_intrinsics.js");
    let js_text = fs::read_to_string(&js_module).expect("read JS module");
    assert!(
        js_text.contains(r#"return "  hello  ".trim();"#),
        "{js_text}"
    );
    assert!(
        js_text.contains(r#"return "hello".toUpperCase();"#),
        "{js_text}"
    );
    assert!(
        js_text.contains(r#"return "hello".endsWith("lo");"#),
        "{js_text}"
    );

    let manifest_text =
        fs::read_to_string(js_root.join("manifest.json")).expect("read JS manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse JS manifest");
    assert_eq!(manifest["schema"], "terlan-js-build-v1");
    assert_eq!(
        manifest["modules"][0]["relative_path"],
        "modules/build_single_file_js_string_intrinsics.js"
    );
    assert_runtime_smoke_status(&manifest["modules"][0]["runtime_smoke_status"]);
}

/// Verifies JS builds emit TypeScript declarations when requested.
///
/// Inputs:
/// - A standalone Terlan source file with one public function.
/// - An explicit JavaScript build target, `--declarations`, and isolated
///   output directory.
///
/// Output:
/// - Test passes when `.js` and `.d.ts` artifacts are written side by side and
///   the JS manifest records the declaration path.
///
/// Transformation:
/// - Runs `terlc build --target js --declarations`, then verifies declaration
///   text is derived from CoreIR public function metadata.
#[test]
fn build_command_emits_js_declarations_when_requested() {
    let dir = make_temp_dir("single_file_js_declarations");
    let source_path = dir.join("build_single_file_js_declarations.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module build_single_file_js_declarations.\n\npub add(x: Int, y: Int): Int ->\n    x + y.\n",
    )
    .expect("failed to write source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_path.display().to_string(),
            "--target".to_string(),
            "js".to_string(),
            "--declarations".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_root = out_dir.join("js");
    let declaration_path = js_root.join("modules/build_single_file_js_declarations.d.ts");
    assert!(
        declaration_path.exists(),
        "expected TypeScript declaration at {declaration_path:?}"
    );
    let declaration_text =
        fs::read_to_string(&declaration_path).expect("read TypeScript declaration");
    assert!(
        declaration_text.contains("export function add(x: number, y: number): number;"),
        "{declaration_text}"
    );

    let manifest_text =
        fs::read_to_string(js_root.join("manifest.json")).expect("read JS manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse JS manifest");
    assert_eq!(
        manifest["modules"][0]["declaration_relative_path"],
        "modules/build_single_file_js_declarations.d.ts"
    );
    assert_eq!(
        manifest["modules"][0]["declaration_path"],
        declaration_path.to_string_lossy().to_string()
    );
}

/// Verifies JavaScript builds reject unsupported direct-backend shapes before write.
///
/// Inputs:
/// - A standalone Terlan source file with a public body that the release JS
///   backend currently cannot lower through direct Oxc AST emission.
/// - An explicit JavaScript build target and isolated output directory.
///
/// Output:
/// - Test passes when the build fails and no partial JS module or manifest is
///   written.
///
/// Transformation:
/// - Runs `terlc build --target js` through the normal command path and checks
///   J0.3's direct-backend artifact-write boundary.
#[test]
fn build_command_rejects_unsupported_js_direct_backend_before_artifact_write() {
    let dir = make_temp_dir("single_file_js_direct_reject");
    let source_path = dir.join("build_js_direct_reject.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "\
module build_js_direct_reject.

pub choose(flag: Bool): Int ->
    if { flag -> 1 }.
",
    )
    .expect("failed to write source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_path.display().to_string(),
            "--target".to_string(),
            "js".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::from(1));
    let js_root = out_dir.join("js");
    assert!(
        !js_root.join("modules/build_js_direct_reject.js").exists(),
        "unsupported JS bodies must fail before module artifact write"
    );
    assert!(
        !js_root.join("manifest.json").exists(),
        "unsupported JS bodies must fail before manifest write"
    );
}

/// Verifies semicolon expression sequences execute in manifest entrypoints.
///
/// Inputs:
/// - A manifest-backed project with `app.Main.main/0`.
/// - A `main` body containing multiple semicolon-separated expressions.
///
/// Output:
/// - Test passes when the generated launcher prints both lines in order.
///
/// Transformation:
/// - Builds the project, runs the emitted launcher, and proves expression
///   sequencing lowers into executable Erlang rather than dropping intermediate
///   calls.
#[test]
fn build_command_compiles_semicolon_expression_sequence_entrypoint() {
    let dir = make_temp_dir("semicolon_sequence_project");
    let project_dir = dir.join("app");
    let app_dir = project_dir.join("src").join("app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create source dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "module app.Main.\n\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(\"Hello Terlan\");\n    println(\"Hello Terlan\").\n",
    )
    .expect("failed to write semicolon sequence module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main.erl");
    assert!(
        erl_text.contains("begin io:format"),
        "sequence should lower to an Erlang begin block:\n{}",
        erl_text
    );

    let launcher_output = Command::new(out_dir.join("bin/app"))
        .output()
        .expect("run semicolon sequence launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&launcher_output.stdout),
        "Hello Terlan\nHello Terlan\n"
    );
}

/// Verifies directory builds emit Erlang source and BEAM bytecode for each
/// package-rooted module.
///
/// Inputs:
/// - A directory containing multiple Terlan modules.
/// - An explicit Erlang build target and isolated output directory.
///
/// Output:
/// - Test passes when all expected Erlang source and BEAM files are emitted and
///   no package launcher is written for directory-only builds.
///
/// Transformation:
/// - Runs directory build discovery and validates emitted module files plus the
///   debug map module/source-path/CoreIR-hash records.
#[test]
fn build_command_emits_erlang_sources_and_beams_for_directory() {
    let dir = make_temp_dir("directory");
    let source_dir = dir.join("project");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::write(
        source_dir.join("a_user.terl"),
        "module a_user.\n\nimport z_dep.{add}.\n\npub value(): Int ->\n    add(1).\n",
    )
    .expect("failed to write user source fixture");
    fs::write(
        source_dir.join("z_dep.terl"),
        "module z_dep.\n\npub add(x: Int): Int ->\n    x + 1.\n",
    )
    .expect("failed to write dependency source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(out_dir.join("src/a_user.erl").exists());
    assert!(out_dir.join("src/z_dep.erl").exists());
    assert!(out_dir.join("ebin/a_user.beam").exists());
    assert!(out_dir.join("ebin/z_dep.beam").exists());

    let debug_map_text = fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE))
        .expect("read directory build debug map");
    let debug_map: serde_json::Value =
        serde_json::from_str(&debug_map_text).expect("parse directory build debug map");
    assert_eq!(debug_map["schema"], BUILD_DEBUG_MAP_SCHEMA);
    assert_eq!(debug_map["target"], "erlang");
    let modules = debug_map["modules"].as_array().expect("modules");
    let module_names = modules
        .iter()
        .map(|entry| entry["module"].as_str().expect("module name"))
        .collect::<Vec<_>>();
    assert_eq!(module_names, vec!["a_user", "z_dep"]);
    assert_eq!(
        modules[0]["erl_path"],
        out_dir.join("src/a_user.erl").to_string_lossy().to_string()
    );
    assert_eq!(
        modules[1]["beam_path"],
        out_dir
            .join("ebin/z_dep.beam")
            .to_string_lossy()
            .to_string()
    );
}

/// Verifies directory JavaScript builds emit one JS module per source file.
///
/// Inputs:
/// - A source directory containing multiple package-rooted Terlan modules.
/// - An explicit JavaScript build target and isolated output directory.
///
/// Output:
/// - Test passes when all expected `.js` modules are emitted and listed in the
///   JS build manifest without Erlang artifacts.
///
/// Transformation:
/// - Runs source-root discovery through `terlc build --target js`, then checks
///   that the J0.1 JS layout receives deterministic module artifacts.
#[test]
fn build_command_emits_js_modules_and_manifest_for_directory() {
    let dir = make_temp_dir("directory_js");
    let source_dir = dir.join("project");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::write(
        source_dir.join("a_math.terl"),
        "module a_math.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write first source fixture");
    fs::write(
        source_dir.join("z_math.terl"),
        "module z_math.\n\npub add(x: Int): Int ->\n    x + 1.\n",
    )
    .expect("failed to write second source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_dir.display().to_string(),
            "--target".to_string(),
            "js".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_root = out_dir.join("js");
    assert!(js_root.join("modules/a_math.js").exists());
    assert!(js_root.join("modules/z_math.js").exists());
    assert!(
        !out_dir.join("src/a_math.erl").exists(),
        "JS directory builds should not emit Erlang source"
    );
    assert!(
        !out_dir.join("ebin/z_math.beam").exists(),
        "JS directory builds should not emit BEAM bytecode"
    );

    let manifest_text =
        fs::read_to_string(js_root.join("manifest.json")).expect("read JS directory manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse JS directory manifest");
    assert_eq!(manifest["schema"], "terlan-js-build-v1");
    assert_eq!(manifest["target_profile"], "js.shared");
    let modules = manifest["modules"].as_array().expect("modules");
    let module_names = modules
        .iter()
        .map(|entry| entry["module"].as_str().expect("module name"))
        .collect::<Vec<_>>();
    assert_eq!(module_names, vec!["a_math", "z_math"]);
    for module in modules {
        assert_runtime_smoke_status(&module["runtime_smoke_status"]);
    }
}

/// Verifies browser JavaScript builds package runnable web artifacts.
///
/// Inputs:
/// - A source directory containing multiple Terlan modules.
/// - An explicit `js.browser` build target and isolated output directory.
///
/// Output:
/// - Test passes when the normal JS module layout is preserved and `_build/web`
///   receives copied JS assets, `index.html`, and a browser package manifest.
///
/// Transformation:
/// - Runs the release-facing JS browser build path and checks that browser
///   packaging remains deterministic glue over Oxc-validated JS modules rather
///   than a separate bundler implementation.
#[test]
fn build_command_emits_browser_web_package_for_js_browser_target() {
    let dir = make_temp_dir("directory_js_browser_package");
    let source_dir = dir.join("project");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::create_dir_all(source_dir.join("assets")).expect("failed to create asset dir");
    fs::write(
        source_dir.join("assets/app.css"),
        "body { color: black; }\n",
    )
    .expect("failed to write css fixture");
    fs::write(source_dir.join("assets/logo.txt"), "terlan\n")
        .expect("failed to write file fixture");
    fs::write(source_dir.join("assets/post.md"), "# Terlan\n")
        .expect("failed to write markdown fixture");
    fs::write(
        source_dir.join("app.terl"),
        r#"module app.

import css "./assets/app.css" as AppCss.
import file "./assets/logo.txt" as Logo.
import markdown "./assets/post.md" as Post.

pub value(): Int ->
    1.
"#,
    )
    .expect("failed to write app source fixture");
    fs::write(
        source_dir.join("helper.terl"),
        "module helper.\n\npub add(x: Int): Int ->\n    x + 1.\n",
    )
    .expect("failed to write helper source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_dir.display().to_string(),
            "--target".to_string(),
            "js.browser".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_root = out_dir.join("js");
    assert!(js_root.join("modules/app.js").exists());
    assert!(js_root.join("modules/helper.js").exists());

    let web_root = out_dir.join("web");
    assert!(web_root.join("index.html").exists());
    assert!(web_root.join("manifest.json").exists());
    assert!(web_root.join("assets/js/modules/app.js").exists());
    assert!(web_root.join("assets/js/modules/helper.js").exists());
    assert!(
        web_root.join("assets/imports/app").exists(),
        "expected imported app asset directory"
    );

    let index_html = fs::read_to_string(web_root.join("index.html")).expect("read web index");
    assert!(
        index_html.contains(r#"<script type="module" src="./assets/js/modules/app.js"></script>"#),
        "{index_html}"
    );
    assert!(
        index_html
            .contains(r#"<script type="module" src="./assets/js/modules/helper.js"></script>"#),
        "{index_html}"
    );

    let manifest_text =
        fs::read_to_string(web_root.join("manifest.json")).expect("read web manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse web manifest");
    assert_eq!(manifest["schema"], "terlan-web-build-v1");
    assert_eq!(manifest["target_profile"], "js.browser");
    assert_eq!(manifest["source_js_manifest"], "../js/manifest.json");
    assert_eq!(manifest["index"], "index.html");
    let assets = manifest["assets"].as_array().expect("assets");
    let asset_paths = assets
        .iter()
        .map(|entry| entry["web_relative_path"].as_str().expect("asset path"))
        .collect::<Vec<_>>();
    assert_eq!(asset_paths.first(), Some(&"assets/js/modules/app.js"));
    assert_eq!(asset_paths.last(), Some(&"assets/js/modules/helper.js"));
    assert!(asset_paths
        .iter()
        .any(|path| { path.starts_with("assets/imports/app/AppCss-") && path.ends_with(".css") }));
    assert!(asset_paths
        .iter()
        .any(|path| { path.starts_with("assets/imports/app/Logo-") && path.ends_with(".txt") }));
    assert!(asset_paths
        .iter()
        .any(|path| { path.starts_with("assets/imports/app/Post-") && path.ends_with(".md") }));
    for path in &asset_paths {
        assert!(
            web_root.join(path).exists(),
            "expected copied browser asset at {path}"
        );
    }
    let asset_kinds = assets
        .iter()
        .map(|entry| entry["kind"].as_str().expect("asset kind"))
        .collect::<Vec<_>>();
    assert_eq!(
        asset_kinds,
        vec![
            "javascript-module",
            "asset-css",
            "asset-file",
            "asset-markdown",
            "javascript-module"
        ]
    );
    let asset_sources = assets
        .iter()
        .map(|entry| {
            entry["source_relative_path"]
                .as_str()
                .expect("asset source")
        })
        .collect::<Vec<_>>();
    assert!(asset_sources.contains(&"./assets/app.css"));
    assert!(asset_sources.contains(&"./assets/logo.txt"));
    assert!(asset_sources.contains(&"./assets/post.md"));
    for asset in assets {
        assert!(
            asset["fingerprint"].as_u64().expect("asset fingerprint") > 0,
            "{asset:?}"
        );
    }
}

/// Verifies manifest-declared web assets are copied into browser packages.
///
/// Inputs:
/// - A manifest-backed project with `[web.assets] directory = "assets"`.
/// - An explicit `js.browser` build target and isolated output directory.
///
/// Output:
/// - Test passes when files under the manifest asset directory are copied into
///   `_build/web/assets` and recorded as `static-asset` manifest rows.
///
/// Transformation:
/// - Runs the project JS browser build path so parsed `terlan.toml` metadata is
///   carried through source-root resolution into browser package emission.
#[test]
fn build_command_emits_manifest_declared_static_assets_for_js_browser_project() {
    let dir = make_temp_dir("directory_js_browser_manifest_assets");
    let project_dir = dir.join("project");
    let source_dir = project_dir.join("src/demo");
    let asset_dir = project_dir.join("assets/nested");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::create_dir_all(&asset_dir).expect("failed to create asset dir");
    fs::write(
        project_dir.join("terlan.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.4\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n\n[web.assets]\ndirectory = \"assets\"\npublic_path = \"/assets\"\n",
    )
    .expect("failed to write manifest");
    fs::write(asset_dir.join("logo.txt"), "terlan\n").expect("failed to write asset");
    fs::write(
        source_dir.join("Main.terl"),
        "module demo.Main.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "js.browser".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let web_root = out_dir.join("web");
    assert!(web_root.join("assets/nested/logo.txt").exists());
    let manifest_text =
        fs::read_to_string(web_root.join("manifest.json")).expect("read web manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse web manifest");
    let assets = manifest["assets"].as_array().expect("assets");
    let static_asset = assets
        .iter()
        .find(|entry| entry["kind"] == "static-asset")
        .expect("static asset manifest row");
    assert_eq!(
        static_asset["source_relative_path"],
        "assets/nested/logo.txt"
    );
    assert_eq!(static_asset["web_relative_path"], "assets/nested/logo.txt");
    assert!(static_asset["fingerprint"].as_u64().expect("fingerprint") > 0);
}

/// Verifies directory builds recursively discover package-rooted source
/// layouts.
///
/// Inputs:
/// - A nested `std/core/Bool.terl` provider module.
/// - A nested `app/Main.terl` consumer module importing the provider through
///   its dotted module identity.
///
/// Output:
/// - Test passes when `terlc build <dir> --target erlang` emits Erlang
///   source and BEAM artifacts for both nested source files.
///
/// Transformation:
/// - Runs recursive source discovery, directory interface-cache
///   validation, CoreIR lowering, Erlang source emission, and `erlc` so
///   package-rooted layouts are proven at artifact level.
#[test]
fn build_command_emits_erlang_sources_and_beams_for_recursive_package_layout() {
    let dir = make_temp_dir("directory_recursive_package_layout");
    let source_dir = dir.join("project");
    let app_dir = source_dir.join("app");
    let std_core_dir = source_dir.join("std/core");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create app source dir");
    fs::create_dir_all(&std_core_dir).expect("failed to create std core source dir");
    fs::write(
        app_dir.join("Main.terl"),
        "module app.Main.\n\nimport std.core.Bool.{truth}.\n\npub value(): Bool ->\n    truth().\n",
    )
    .expect("failed to write nested app source fixture");
    fs::write(
        std_core_dir.join("Bool.terl"),
        "module std.core.Bool.\n\npub truth(): Bool ->\n    true.\n",
    )
    .expect("failed to write nested std core source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(out_dir.join("src/app_main.erl").exists());
    assert!(out_dir.join("src/std_core_bool.erl").exists());
    assert!(out_dir.join("ebin/app_main.beam").exists());
    assert!(out_dir.join("ebin/std_core_bool.beam").exists());

    let debug_map_text = fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE))
        .expect("read recursive directory build debug map");
    let debug_map: serde_json::Value =
        serde_json::from_str(&debug_map_text).expect("parse recursive build debug map");
    assert_eq!(debug_map["schema"], BUILD_DEBUG_MAP_SCHEMA);
    assert_eq!(debug_map["target"], "erlang");
    let modules = debug_map["modules"].as_array().expect("modules");
    let module_names = modules
        .iter()
        .map(|entry| entry["module"].as_str().expect("module name"))
        .collect::<Vec<_>>();
    assert_eq!(module_names, vec!["app.Main", "std.core.Bool"]);
    assert_eq!(
        modules[0]["source_path"],
        app_dir.join("Main.terl").to_string_lossy().to_string()
    );
    assert_eq!(
        modules[0]["erl_path"],
        out_dir
            .join("src/app_main.erl")
            .to_string_lossy()
            .to_string()
    );
    assert_eq!(
        modules[0]["beam_path"],
        out_dir
            .join("ebin/app_main.beam")
            .to_string_lossy()
            .to_string()
    );
    assert!(
        modules[0]["core_ir_hash"].as_u64().expect("app hash") > 0,
        "app module should carry a nonzero CoreIR hash"
    );
    assert_eq!(
        modules[1]["source_path"],
        std_core_dir.join("Bool.terl").to_string_lossy().to_string()
    );
    assert_eq!(
        modules[1]["erl_path"],
        out_dir
            .join("src/std_core_bool.erl")
            .to_string_lossy()
            .to_string()
    );
    assert_eq!(
        modules[1]["beam_path"],
        out_dir
            .join("ebin/std_core_bool.beam")
            .to_string_lossy()
            .to_string()
    );
    assert!(
        modules[1]["core_ir_hash"].as_u64().expect("std hash") > 0,
        "std.core module should carry a nonzero CoreIR hash"
    );
}

/// Verifies directory builds compile recursive type-only and value imports.
///
/// Inputs:
/// - A nested `std/core/UserId.terl` provider exporting a public type alias
///   and a public constructor-like helper function.
/// - A nested `app/User.terl` consumer importing the provider type through
///   `import type` and the helper through a selected value import.
///
/// Output:
/// - Test passes when `terlc build <dir> --target erlang` emits Erlang
///   source and BEAM artifacts for both nested modules and records both
///   modules in the build debug map.
///
/// Transformation:
/// - Runs recursive directory discovery, interface-cache dependency
///   closure, type-only import resolution, selected value import
///   resolution, CoreIR lowering, Erlang source emission, and `erlc` so
///   package-rooted type/value import closure is proven at artifact level.
#[test]
fn build_command_compiles_recursive_type_and_value_import_dependency_closure() {
    let dir = make_temp_dir("directory_recursive_type_and_value_imports");
    let source_dir = dir.join("project");
    let app_dir = source_dir.join("app");
    let std_core_dir = source_dir.join("std/core");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create app source dir");
    fs::create_dir_all(&std_core_dir).expect("failed to create std core source dir");
    fs::write(
        app_dir.join("User.terl"),
        "module app.User.\n\nimport type std.core.UserId.UserId.\nimport std.core.UserId.{from_int}.\n\npub default_id(): UserId ->\n    from_int(1).\n",
    )
    .expect("failed to write recursive type/value import consumer");
    fs::write(
        std_core_dir.join("UserId.terl"),
        "module std.core.UserId.\n\npub type UserId = Int.\n\npub from_int(value: Int): UserId ->\n    value.\n",
    )
    .expect("failed to write recursive type/value import provider");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(out_dir.join("src/app_user.erl").exists());
    assert!(out_dir.join("src/std_core_userid.erl").exists());
    assert!(out_dir.join("ebin/app_user.beam").exists());
    assert!(out_dir.join("ebin/std_core_userid.beam").exists());

    let debug_map_text = fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE))
        .expect("read type/value import directory build debug map");
    let debug_map: serde_json::Value =
        serde_json::from_str(&debug_map_text).expect("parse type/value import debug map");
    let modules = debug_map["modules"].as_array().expect("modules");
    let module_names = modules
        .iter()
        .map(|entry| entry["module"].as_str().expect("module name"))
        .collect::<Vec<_>>();
    assert_eq!(module_names, vec!["app.User", "std.core.UserId"]);
    assert_eq!(
        modules[0]["source_path"],
        app_dir.join("User.terl").to_string_lossy().to_string()
    );
    assert_eq!(
        modules[1]["source_path"],
        std_core_dir
            .join("UserId.terl")
            .to_string_lossy()
            .to_string()
    );
    assert!(
        modules
            .iter()
            .all(|entry| entry["core_ir_hash"].as_u64().expect("core hash") > 0),
        "all dependency-closure modules should carry nonzero CoreIR hashes"
    );
}
