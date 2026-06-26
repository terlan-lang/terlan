use super::*;

/// Verifies project builds include local path dependency source roots.
///
/// Inputs:
/// - A root project manifest with a local `[dependencies]` path entry.
/// - A dependency project with its own manifest and source root.
/// - A root source file that imports a value from the dependency source.
///
/// Output:
/// - Test passes when both dependency and root modules emit Erlang source
///   and BEAM artifacts through one project build.
///
/// Transformation:
/// - Resolves the local path dependency manifest before backend emission,
///   validates dependency source roots before the root source root, and
///   emits the ordered package closure through the existing build path.
#[test]
fn build_command_compiles_project_with_local_path_dependency() {
    let dir = make_temp_dir("project_local_path_dependency");
    let app_dir = dir.join("app");
    let dep_dir = dir.join("local_utils");
    let app_src = app_dir.join("src/app");
    let dep_src = dep_dir.join("src/local_utils");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_src).expect("failed to create app src dir");
    fs::create_dir_all(&dep_src).expect("failed to create dependency src dir");
    fs::write(
            app_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n\n[dependencies]\nlocal_utils = { path = \"../local_utils\" }\n",
        )
        .expect("failed to write app manifest");
    fs::write(
            dep_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"local_utils\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n\n[native.rust]\ncrate = \"local_utils_native\"\npath = \"native\"\nhelper = \"local-utils-safe-native\"\nhelper_env = \"LOCAL_UTILS_SAFE_NATIVE_PATH\"\n",
        )
        .expect("failed to write dependency manifest");
    fs::write(
        dep_src.join("Util.terl"),
        "module local_utils.Util.\n\npub one(): Int ->\n    1.\n",
    )
    .expect("failed to write dependency module");
    fs::write(
            app_src.join("Main.terl"),
            "module app.Main.\n\nimport local_utils.Util.{one}.\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(\"ok\");\n    Unit.\n\npub value(): Int ->\n    one().\n",
        )
        .expect("failed to write app module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            app_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(out_dir.join("src/local_utils_util.erl").exists());
    assert!(out_dir.join("src/app_main.erl").exists());
    assert!(out_dir.join("ebin/local_utils_util.beam").exists());
    assert!(out_dir.join("ebin/app_main.beam").exists());

    let debug_map_text = fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE))
        .expect("read local dependency project debug map");
    let debug_map: serde_json::Value =
        serde_json::from_str(&debug_map_text).expect("parse local dependency debug map");
    let modules = debug_map["modules"].as_array().expect("modules");
    let module_names = modules
        .iter()
        .map(|entry| entry["module"].as_str().expect("module name"))
        .collect::<Vec<_>>();
    assert_eq!(module_names, vec!["local_utils.Util", "app.Main"]);

    let package_metadata_text = fs::read_to_string(out_dir.join(BUILD_PACKAGE_METADATA_FILE))
        .expect("read local dependency package metadata");
    let package_metadata: serde_json::Value = serde_json::from_str(&package_metadata_text)
        .expect("parse local dependency package metadata");
    assert_eq!(package_metadata["schema"], BUILD_PACKAGE_METADATA_SCHEMA);
    assert_eq!(package_metadata["package"]["name"], "app");
    let dependencies = package_metadata["dependencies"]
        .as_array()
        .expect("package dependencies");
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0]["alias"], "local_utils");
    assert_eq!(dependencies[0]["scope"], "local");
    assert_eq!(dependencies[0]["source"], "path");
    assert_eq!(dependencies[0]["path"], "../local_utils");
    assert!(dependencies[0].get("package").is_none());
    assert!(dependencies[0].get("version").is_none());
    let native_dependencies = package_metadata["native"]["rust_dependencies"]
        .as_array()
        .expect("native rust dependencies");
    assert_eq!(native_dependencies.len(), 1);
    assert_eq!(native_dependencies[0]["package"], "local_utils");
    assert_eq!(
        native_dependencies[0]["rust"]["crate"],
        "local_utils_native"
    );
    assert_eq!(native_dependencies[0]["rust"]["path"], "native");
    assert_eq!(
        native_dependencies[0]["rust"]["helper"],
        "local-utils-safe-native"
    );
    assert_eq!(
        native_dependencies[0]["rust"]["helper_env"],
        "LOCAL_UTILS_SAFE_NATIVE_PATH"
    );
    assert_eq!(
        native_dependencies[0]["rust"]["package_dir"],
        dep_dir
            .canonicalize()
            .expect("canonical dependency dir")
            .display()
            .to_string()
    );
}

/// Verifies local path dependencies require their own manifest.
///
/// Inputs:
/// - A root project with a local `path` dependency.
/// - A dependency directory without `terlan.toml`.
///
/// Output:
/// - Test passes when build fails before generated artifacts are written.
///
/// Transformation:
/// - Resolves local dependency metadata, checks for the dependency
///   manifest, and rejects the project before source-root validation or
///   backend emission can run.
#[test]
fn build_command_rejects_local_path_dependency_without_manifest() {
    let dir = make_temp_dir("project_local_path_dependency_missing_manifest");
    let app_dir = dir.join("app");
    let dep_dir = dir.join("local_utils");
    let app_src = app_dir.join("src");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_src).expect("failed to create app src dir");
    fs::create_dir_all(&dep_dir).expect("failed to create dependency dir");
    fs::write(
            app_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[dependencies]\nlocal_utils = { path = \"../local_utils\" }\n",
        )
        .expect("failed to write app manifest");
    fs::write(
        app_src.join("main.terl"),
        "module main.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write app module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            app_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::from(1));
    assert!(!out_dir.join("src/main.erl").exists());
    assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
}

/// Verifies local path dependency cycles fail before backend emission.
///
/// Inputs:
/// - Two project manifests that depend on each other through local `path`
///   dependencies.
///
/// Output:
/// - Test passes when the build fails and no backend artifacts are written.
///
/// Transformation:
/// - Tracks packages currently being resolved and rejects a dependency path
///   that re-enters the active resolution stack.
#[test]
fn build_command_rejects_local_path_dependency_cycle() {
    let dir = make_temp_dir("project_local_path_dependency_cycle");
    let app_dir = dir.join("app");
    let dep_dir = dir.join("local_utils");
    let app_src = app_dir.join("src");
    let dep_src = dep_dir.join("src");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_src).expect("failed to create app src dir");
    fs::create_dir_all(&dep_src).expect("failed to create dependency src dir");
    fs::write(
            app_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[dependencies]\nlocal_utils = { path = \"../local_utils\" }\n",
        )
        .expect("failed to write app manifest");
    fs::write(
            dep_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"local_utils\"\nversion = \"0.0.1\"\n\n[dependencies]\napp = { path = \"../app\" }\n",
        )
        .expect("failed to write dependency manifest");
    fs::write(
        app_src.join("main.terl"),
        "module main.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write app module");
    fs::write(
        dep_src.join("util.terl"),
        "module util.\n\npub one(): Int ->\n    1.\n",
    )
    .expect("failed to write dependency module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            app_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::from(1));
    assert!(!out_dir.join("src/main.erl").exists());
    assert!(!out_dir.join("src/util.erl").exists());
    assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
}

/// Verifies Hex dependency metadata is rejected before backend emission.
///
/// Inputs:
/// - A project manifest with `[target.erlang.dependencies]`.
/// - A buildable source root.
///
/// Output:
/// - Test passes when build exits with failure and writes no artifacts.
///
/// Transformation:
/// - Parses the target-scoped dependency metadata, detects unsupported Hex
///   package-manager integration, and stops before source-root emission.
#[test]
fn build_command_rejects_hex_dependency_metadata_before_emission() {
    let dir = make_temp_dir("project_hex_dependency_metadata");
    let project_dir = dir.join("project");
    let source_dir = project_dir.join("src");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.erlang.dependencies]\ncowboy = { hex = \"cowboy\", version = \"2.12.0\" }\n",
        )
        .expect("failed to write project manifest");
    fs::write(
        source_dir.join("main.terl"),
        "module main.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write project module");

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

    assert_eq!(status, ExitCode::from(1));
    assert!(!out_dir.join("src/main.erl").exists());
    assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
}

/// Verifies npm dependency metadata is rejected before backend emission.
///
/// Inputs:
/// - A project manifest with `[target.js.dependencies]`.
/// - A buildable source root.
///
/// Output:
/// - Test passes when build exits with failure and writes no artifacts.
///
/// Transformation:
/// - Parses the target-scoped dependency metadata, detects unsupported npm
///   package-manager integration, and stops before source-root emission.
#[test]
fn build_command_rejects_npm_dependency_metadata_before_emission() {
    let dir = make_temp_dir("project_npm_dependency_metadata");
    let project_dir = dir.join("project");
    let source_dir = project_dir.join("src");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.js.dependencies]\nzod = { npm = \"zod\", version = \"3.25.0\" }\n",
        )
        .expect("failed to write project manifest");
    fs::write(
        source_dir.join("main.terl"),
        "module main.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write project module");

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

    assert_eq!(status, ExitCode::from(1));
    assert!(!out_dir.join("src/main.erl").exists());
    assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
}

/// Verifies cargo dependency metadata is rejected before backend emission.
///
/// Inputs:
/// - A project manifest with `[target.rust.dependencies]`.
/// - A buildable source root.
///
/// Output:
/// - Test passes when build exits with failure and writes no artifacts.
///
/// Transformation:
/// - Parses the target-scoped dependency metadata, detects unsupported
///   Cargo package-manager integration, and stops before source-root
///   emission.
#[test]
fn build_command_rejects_cargo_dependency_metadata_before_emission() {
    let dir = make_temp_dir("project_cargo_dependency_metadata");
    let project_dir = dir.join("project");
    let source_dir = project_dir.join("src");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.rust.dependencies]\nserde = { cargo = \"serde\", version = \"1.0.0\" }\n",
        )
        .expect("failed to write project manifest");
    fs::write(
        source_dir.join("main.terl"),
        "module main.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write project module");

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

    assert_eq!(status, ExitCode::from(1));
    assert!(!out_dir.join("src/main.erl").exists());
    assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
}
