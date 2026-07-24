
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
    assert!(
        !out_dir.join("mobile").exists(),
        "ordinary js.browser builds must not emit mobile shell artifacts"
    );

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
        index_html.contains(
            r#"<script type="module" src="./assets/js/modules/app.js" integrity="sha256-"#
        ),
        "{index_html}"
    );
    assert!(
        index_html.contains(
            r#"<script type="module" src="./assets/js/modules/helper.js" integrity="sha256-"#
        ),
        "{index_html}"
    );

    let manifest_text =
        fs::read_to_string(web_root.join("manifest.json")).expect("read web manifest");
    assert!(
        !manifest_text.contains(r"assets\\"),
        "browser package paths must use portable separators: {manifest_text}"
    );
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse web manifest");
    assert_eq!(manifest["schema"], "terlan-web-build-v1");
    assert_eq!(manifest["target_profile"], "js.browser");
    assert!(
        manifest["build_id"]
            .as_str()
            .expect("build id")
            .starts_with("web-"),
        "{manifest_text}"
    );
    assert_eq!(manifest["source_js_manifest"], "../js/manifest.json");
    assert_eq!(manifest["index"], "index.html");
    let assets = manifest["assets"].as_array().expect("assets");
    let asset_paths = assets
        .iter()
        .map(|entry| entry["web_relative_path"].as_str().expect("asset path"))
        .collect::<Vec<_>>();
    assert_eq!(asset_paths.first(), Some(&"assets/js/modules/app.js"));
    assert!(asset_paths.contains(&"assets/js/modules/app.js.map"));
    assert!(asset_paths.contains(&"assets/js/modules/helper.js"));
    assert!(asset_paths.contains(&"assets/js/modules/helper.js.map"));
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
            "javascript-source-map",
            "asset-css",
            "asset-file",
            "asset-markdown",
            "javascript-module",
            "javascript-source-map"
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
        assert!(
            asset["integrity"]
                .as_str()
                .expect("asset integrity")
                .starts_with("sha256-"),
            "{asset:?}"
        );
    }
    let app_js = fs::read_to_string(web_root.join("assets/js/modules/app.js"))
        .expect("read browser JS with source map link");
    assert!(
        app_js.contains("//# sourceMappingURL=app.js.map"),
        "{app_js}"
    );
    let source_map_text = fs::read_to_string(web_root.join("assets/js/modules/app.js.map"))
        .expect("read browser JS source map");
    let source_map: serde_json::Value =
        serde_json::from_str(&source_map_text).expect("parse browser JS source map");
    assert_eq!(source_map["version"], 3);
    assert_eq!(source_map["file"], "app.js");
    assert_eq!(source_map["sources"][0], "app.terl");
    assert!(
        !source_map_text.contains(dir.to_string_lossy().as_ref()),
        "{source_map_text}"
    );
}

/// Verifies browser JavaScript builds are inferred from source evidence.
///
/// Inputs:
/// - A source directory containing CSS/file asset imports.
/// - No explicit `--target` argument.
///
/// Output:
/// - Test passes when the build emits the browser web package instead of a VM
///   artifact.
///
/// Transformation:
/// - Runs target inference before backend dispatch so browser package evidence
///   comes from typed source imports rather than manual CLI flags.
#[test]
fn build_command_infers_js_browser_target_from_asset_imports() {
    let dir = make_temp_dir("directory_js_browser_inferred");
    let source_dir = dir.join("project");
    let out_dir = dir.join("build");
    fs::create_dir_all(source_dir.join("assets")).expect("failed to create asset dir");
    fs::write(
        source_dir.join("assets/app.css"),
        "body { color: black; }\n",
    )
    .expect("failed to write css fixture");
    fs::write(source_dir.join("assets/logo.txt"), "terlan\n")
        .expect("failed to write file fixture");
    fs::write(
        source_dir.join("app.terl"),
        r#"module app.

import css "./assets/app.css" as AppCss.
import file "./assets/logo.txt" as Logo.

pub value(): Int ->
    1.
"#,
    )
    .expect("failed to write app source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![source_dir.display().to_string()],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(out_dir.join("web/index.html").exists());
    assert!(out_dir.join("js/modules/app.js").exists());
    assert!(
        !out_dir.join("vm/app.tvm").exists(),
        "browser target inference must not emit a VM artifact"
    );
}

/// Verifies explicit VM target overrides cannot hide JavaScript evidence.
///
/// Inputs:
/// - A source file importing `std.js.Promise`.
/// - Explicit `--target terlan-vm`.
///
/// Output:
/// - Test passes when build exits with a target-evidence diagnostic before
///   emitting artifacts.
///
/// Transformation:
/// - Keeps CLI target flags as checked overrides while preserving source
///   evidence as the compatibility authority.
#[test]
fn build_command_rejects_explicit_vm_target_for_js_evidence() {
    let dir = make_temp_dir("single_file_js_inference_conflict");
    let source_path = dir.join("js_conflict.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module js_conflict.\n\nimport std.js.Promise.\n\npub value(): Int ->\n    1.\n",
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
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::from(1));
    assert!(
        !out_dir.exists(),
        "conflicting target build must not emit output"
    );
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
        "[package]\nname = \"demo\"\nversion = \"0.0.4\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n\n[web.assets]\ndirectory = \"assets\"\npublic_path = \"/assets\"\n",
    )
    .expect("failed to write manifest");
    fs::write(asset_dir.join("logo.txt"), "terlan\n").expect("failed to write asset");
    fs::write(asset_dir.join("logo with space.txt"), "terlan spaced\n")
        .expect("failed to write spaced asset");
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
    assert!(web_root.join("assets/nested/logo with space.txt").exists());
    let manifest_text =
        fs::read_to_string(web_root.join("manifest.json")).expect("read web manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse web manifest");
    let assets = manifest["assets"].as_array().expect("assets");
    let static_asset = assets
        .iter()
        .find(|entry| {
            entry["kind"] == "static-asset"
                && entry["web_relative_path"] == "assets/nested/logo.txt"
        })
        .expect("static asset manifest row");
    assert_eq!(
        static_asset["source_relative_path"],
        "assets/nested/logo.txt"
    );
    assert_eq!(static_asset["web_relative_path"], "assets/nested/logo.txt");
    assert!(static_asset["fingerprint"].as_u64().expect("fingerprint") > 0);
    assert!(static_asset["integrity"]
        .as_str()
        .expect("static asset integrity")
        .starts_with("sha256-"));
    let spaced_static_asset = assets
        .iter()
        .find(|entry| {
            entry["kind"] == "static-asset"
                && entry["web_relative_path"] == "assets/nested/logo with space.txt"
        })
        .expect("static asset manifest row for path with spaces");
    assert_eq!(
        spaced_static_asset["source_relative_path"],
        "assets/nested/logo with space.txt"
    );
    assert!(
        spaced_static_asset["fingerprint"]
            .as_u64()
            .expect("spaced asset fingerprint")
            > 0
    );
    assert!(spaced_static_asset["integrity"]
        .as_str()
        .expect("spaced asset integrity")
        .starts_with("sha256-"));
}

/// Verifies manifest-declared assets reject case-folded path collisions.
///
/// Inputs:
/// - A browser project with `logo.txt` and `Logo.txt` in the same asset root.
///
/// Output:
/// - Test passes when the browser build fails before writing a web manifest.
///
/// Transformation:
/// - Exercises the static asset copier's cross-platform path safety check so a
///   package cannot build assets that collide on case-insensitive filesystems.
#[test]
fn build_command_rejects_case_folded_static_asset_collisions_for_js_browser_project() {
    let dir = make_temp_dir("directory_js_browser_manifest_asset_case_collision");
    let project_dir = dir.join("project");
    let source_dir = project_dir.join("src/demo");
    let asset_dir = project_dir.join("assets/nested");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::create_dir_all(&asset_dir).expect("failed to create asset dir");
    fs::write(
        project_dir.join("terlan.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.4\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n\n[web.assets]\ndirectory = \"assets\"\npublic_path = \"/assets\"\n",
    )
    .expect("failed to write manifest");
    fs::write(asset_dir.join("logo.txt"), "lower\n").expect("failed to write lower asset");
    fs::write(asset_dir.join("Logo.txt"), "upper\n").expect("failed to write upper asset");
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

    assert_eq!(status, ExitCode::from(1));
    assert!(
        !out_dir.join("web/manifest.json").exists(),
        "case-folded collision must fail before browser manifest write"
    );
}
