use super::*;

/// Verifies project manifests are rejected before silent source-root builds.
///
/// Inputs:
/// - A directory containing `terlan.toml` and one otherwise buildable
///   source module.
///
/// Output:
/// - Test passes when `terlc build <dir> --target erlang` fails and emits
///   no Erlang source, BEAM artifact, or build debug map.
///
/// Transformation:
/// - Runs the build command against a manifest-bearing directory and proves
///   A0.37 package/project manifest semantics are not silently skipped by
///   the plain recursive source-root build path.
#[test]
fn build_command_rejects_project_manifest_before_silent_directory_scan() {
    let dir = make_temp_dir("directory_project_manifest_rejected");
    let source_dir = dir.join("project");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::write(
        source_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        source_dir.join("main.terl"),
        "module main.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write manifest-bearing source fixture");

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

    assert_eq!(status, ExitCode::from(1));
    assert!(!out_dir.join("src/main.erl").exists());
    assert!(!out_dir.join("ebin/main.beam").exists());
    assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
}

/// Verifies project manifests build from the parsed source root.
///
/// Inputs:
/// - A project root containing `terlan.toml`.
/// - A single manifest-declared `src` source root containing one nested
///   package-rooted module.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits Erlang
///   source, a BEAM artifact, and a debug-map entry for the module under
///   the manifest source root.
///
/// Transformation:
/// - Parses `terlan.toml`, resolves `[build] source_roots`, delegates the
///   selected source root to the existing formal source-root build path,
///   and proves the project root itself is not used as the module layout
///   root.
#[test]
fn build_command_compiles_project_manifest_source_root() {
    let dir = make_temp_dir("directory_project_manifest_source_root");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write project manifest fixture");
    fs::write(
            app_dir.join("Main.terl"),
            "module app.Main.\n\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(\"hello\");\n    Unit.\n",
        )
        .expect("failed to write manifest source-root module");

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
    assert!(out_dir.join("src/app_main.erl").exists());
    assert!(out_dir.join("ebin/app_main.beam").exists());
    let executable_path = out_dir.join("bin/app");
    assert!(executable_path.exists());
    assert_eq!(
            fs::read_to_string(&executable_path).expect("read executable launcher"),
            "#!/usr/bin/env sh\nset -eu\nSCRIPT_DIR=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\nROOT_DIR=$(CDPATH= cd -- \"$SCRIPT_DIR/..\" && pwd)\nexec erl -noshell -pa \"$ROOT_DIR/ebin\" -eval \"case catch app_main:main() of {'EXIT', Reason} -> io:format(standard_error, \\\"terlan entrypoint app.Main.main/0 failed: ~p~n\\\", [Reason]), halt(1); _ -> halt(0) end.\" \"$@\"\n"
        );
    assert_executable_bit(&executable_path);
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "hello\n");

    let debug_map_text = fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE))
        .expect("read project manifest build debug map");
    let debug_map: serde_json::Value =
        serde_json::from_str(&debug_map_text).expect("parse project manifest debug map");
    assert_eq!(debug_map["project"]["package"], "app");
    assert_eq!(debug_map["project"]["version"], "0.0.1");
    assert_eq!(debug_map["project"]["source_roots"][0], "src");
    assert_eq!(debug_map["project"]["artifact"], "beam-thin");
    let modules = debug_map["modules"].as_array().expect("modules");
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0]["module"], "app.Main");
    assert_eq!(
        modules[0]["source_path"],
        app_dir.join("Main.terl").to_string_lossy().to_string()
    );

    let package_metadata_text = fs::read_to_string(out_dir.join(BUILD_PACKAGE_METADATA_FILE))
        .expect("read project package metadata");
    let package_metadata: serde_json::Value =
        serde_json::from_str(&package_metadata_text).expect("parse project package metadata");
    assert_eq!(package_metadata["schema"], BUILD_PACKAGE_METADATA_SCHEMA);
    assert_eq!(package_metadata["target"], "erlang");
    assert_eq!(package_metadata["package"]["name"], "app");
    assert_eq!(package_metadata["package"]["version"], "0.0.1");
    assert_eq!(package_metadata["artifact"], "beam-thin");
    assert_eq!(package_metadata["executable"]["mode"], "beam-thin");
    assert_eq!(package_metadata["executable"]["path"], "bin/app");
    assert_eq!(package_metadata["executable"]["runtime"], "external-erts");
    assert_eq!(
        package_metadata["executable"]["entrypoint"]["module"],
        "app.Main"
    );
    assert_eq!(
        package_metadata["executable"]["entrypoint"]["function"],
        "main"
    );
    assert_eq!(package_metadata["executable"]["entrypoint"]["arity"], 0);
    assert_eq!(package_metadata["source_roots"][0], "src");
    assert!(
        package_metadata["dependencies"]
            .as_array()
            .expect("package dependencies")
            .is_empty(),
        "project without dependency metadata should emit an empty dependency list"
    );
}

/// Verifies manifest-backed library packages do not require an executable
/// entrypoint.
///
/// Inputs:
/// - A project manifest with `[build] artifact = "library"`.
/// - A package-rooted source module that does not define `Main.main`.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits
///   module artifacts and package metadata without writing a launcher.
///
/// Transformation:
/// - Parses the library artifact mode, validates the source root, lowers
///   the module, skips executable entrypoint validation, and records package
///   metadata with no executable entry.
#[test]
fn build_command_compiles_project_manifest_library_without_entrypoint() {
    let dir = make_temp_dir("directory_project_manifest_library");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n",
        )
        .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Util.terl"),
        "module app.Util.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write library source module");

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
    assert!(out_dir.join("src/app_util.erl").exists());
    assert!(out_dir.join("ebin/app_util.beam").exists());
    assert!(!out_dir.join("bin/app").exists());
    let package_metadata_text = fs::read_to_string(out_dir.join(BUILD_PACKAGE_METADATA_FILE))
        .expect("read library package metadata");
    let package_metadata: serde_json::Value =
        serde_json::from_str(&package_metadata_text).expect("parse package metadata");
    assert_eq!(package_metadata["artifact"], "library");
    assert!(package_metadata.get("executable").is_none());
}

/// Verifies manifest package namespaces control source layout.
///
/// Inputs:
/// - A library package named `std-sample-polars` with namespace
///   `std.sample.polars`.
/// - A source file under `src/std/sample/polars`.
///
/// Output:
/// - Test passes when the build accepts the namespace path, emits module
///   artifacts, and records namespace metadata.
///
/// Transformation:
/// - Parses `[package] namespace`, validates source files against that
///   namespace path instead of the package-name-derived root, and preserves
///   the namespace in build metadata.
#[test]
fn build_command_compiles_project_manifest_namespace_layout() {
    let dir = make_temp_dir("directory_project_manifest_namespace_layout");
    let project_dir = dir.join("project");
    let module_dir = project_dir.join("src/std/sample/polars");
    let out_dir = dir.join("build");
    fs::create_dir_all(&module_dir).expect("failed to create namespace source dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"std-sample-polars\"\nversion = \"0.0.4\"\nnamespace = \"std.sample.polars\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n",
        )
        .expect("failed to write project manifest fixture");
    fs::write(
            module_dir.join("DataFrame.terl"),
            "module std.sample.polars.DataFrame.\n\npub opaque type DataFrame.\n\npub height(df: DataFrame): Int ->\n    0.\n",
        )
        .expect("failed to write namespaced module");

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
    assert!(out_dir.join("src/std_sample_polars_dataframe.erl").exists());
    assert!(out_dir
        .join("ebin/std_sample_polars_dataframe.beam")
        .exists());
    assert!(!out_dir.join("bin/std-sample-polars").exists());
    let package_metadata_text = fs::read_to_string(out_dir.join(BUILD_PACKAGE_METADATA_FILE))
        .expect("read namespaced package metadata");
    let package_metadata: serde_json::Value =
        serde_json::from_str(&package_metadata_text).expect("parse package metadata");
    assert_eq!(package_metadata["package"]["name"], "std-sample-polars");
    assert_eq!(
        package_metadata["package"]["namespace"],
        "std.sample.polars"
    );
    assert_eq!(package_metadata["artifact"], "library");
}

/// Verifies manifest-backed builds reject source files outside the package root.
///
/// Inputs:
/// - A project manifest whose package name is `app`.
/// - A source file under `src/other` declaring `module other.Main`.
///
/// Output:
/// - Test passes when build fails before writing Erlang source, BEAM
///   artifacts, debug maps, package metadata, or executable launchers.
///
/// Transformation:
/// - Runs the project build path and proves manifest package identity is
///   enforced before the existing source-root layout and backend gates.
#[test]
fn build_command_rejects_project_source_outside_package_root() {
    let dir = make_temp_dir("directory_project_manifest_package_root_mismatch");
    let project_dir = dir.join("project");
    let other_dir = project_dir.join("src/other");
    let out_dir = dir.join("build");
    fs::create_dir_all(&other_dir).expect("failed to create project src dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write project manifest fixture");
    fs::write(
        other_dir.join("Main.terl"),
        "module other.Main.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write mismatched package-root module");

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
    assert!(!out_dir.join("src/other_main.erl").exists());
    assert!(!out_dir.join("ebin/other_main.beam").exists());
    assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
    assert!(!out_dir.join(BUILD_PACKAGE_METADATA_FILE).exists());
    assert!(!out_dir.join("bin/app").exists());
}

/// Verifies Erlang package adapter metadata remains metadata-only.
///
/// Inputs:
/// - A project manifest reserving the Rebar3-compatible Erlang packaging
///   adapter.
/// - A buildable manifest source root.
///
/// Output:
/// - Test passes when the build succeeds, records adapter metadata in
///   `terlan-package-build.json`, and does not generate Rebar3 files.
///
/// Transformation:
/// - Parses `[target.erlang.package]`, runs the formal project build path,
///   and proves A0.42.6 preserves adapter intent without making Rebar3 part
///   of normal `terlc build --target erlang`.
#[test]
fn build_command_preserves_erlang_package_adapter_metadata_without_rebar3_files() {
    let dir = make_temp_dir("directory_project_manifest_erlang_package_adapter");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n\n[target.erlang.package]\nadapter = \"rebar3-compatible\"\n",
        )
        .expect("failed to write project manifest fixture");
    fs::write(
            app_dir.join("Main.terl"),
            "module app.Main.\n\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(\"ok\");\n    Unit.\n",
        )
        .expect("failed to write manifest source-root module");

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
    assert!(out_dir.join("src/app_main.erl").exists());
    assert!(out_dir.join("ebin/app_main.beam").exists());
    assert!(
        !out_dir.join("rebar.config").exists(),
        "adapter metadata must not generate Rebar3 files in A0.42.6"
    );
    assert!(
        !out_dir.join("src/demo.app.src").exists(),
        "adapter metadata must not generate OTP app metadata in A0.42.6"
    );

    let package_metadata_text = fs::read_to_string(out_dir.join(BUILD_PACKAGE_METADATA_FILE))
        .expect("read project package metadata");
    let package_metadata: serde_json::Value =
        serde_json::from_str(&package_metadata_text).expect("parse project package metadata");
    let adapters = package_metadata["adapters"].as_array().expect("adapters");
    assert_eq!(adapters.len(), 1);
    assert_eq!(adapters[0]["target"], "erlang");
    assert_eq!(adapters[0]["adapter"], "rebar3-compatible");
}

/// Verifies project manifests build multiple declared source roots.
///
/// Inputs:
/// - A project root containing `terlan.toml`.
/// - Two manifest-declared source roots where the second imports a value
///   from the first.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits Erlang
///   sources, BEAM artifacts, and one combined debug map for both roots.
///
/// Transformation:
/// - Parses `terlan.toml`, resolves all `[build] source_roots`, validates
///   each root with a shared interface cache, lowers both roots through
///   CoreIR, and writes one source-to-artifact map across the project.
#[test]
fn build_command_compiles_project_manifest_multiple_source_roots() {
    let dir = make_temp_dir("directory_project_manifest_multiple_source_roots");
    let project_dir = dir.join("project");
    let lib_dir = project_dir.join("lib/demo");
    let app_dir = project_dir.join("app/demo");
    let out_dir = dir.join("build");
    fs::create_dir_all(&lib_dir).expect("failed to create project lib dir");
    fs::create_dir_all(&app_dir).expect("failed to create project app dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"lib\", \"app\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write multi-root project manifest fixture");
    fs::write(
        lib_dir.join("Util.terl"),
        "module demo.Util.\n\npub one(): Int ->\n    1.\n",
    )
    .expect("failed to write multi-root provider module");
    fs::write(
            app_dir.join("Main.terl"),
            "module demo.Main.\n\nimport demo.Util.{one}.\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(\"ok\");\n    Unit.\n\npub value(): Int ->\n    one().\n",
        )
        .expect("failed to write multi-root consumer module");

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
    assert!(out_dir.join("src/demo_util.erl").exists());
    assert!(out_dir.join("src/demo_main.erl").exists());
    assert!(out_dir.join("ebin/demo_util.beam").exists());
    assert!(out_dir.join("ebin/demo_main.beam").exists());

    let debug_map_text = fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE))
        .expect("read multi-root project build debug map");
    let debug_map: serde_json::Value =
        serde_json::from_str(&debug_map_text).expect("parse multi-root project debug map");
    assert_eq!(debug_map["project"]["package"], "demo");
    assert_eq!(debug_map["project"]["version"], "0.0.1");
    assert_eq!(debug_map["project"]["source_roots"][0], "lib");
    assert_eq!(debug_map["project"]["source_roots"][1], "app");
    assert_eq!(debug_map["project"]["artifact"], "beam-thin");
    let modules = debug_map["modules"].as_array().expect("modules");
    let module_names = modules
        .iter()
        .map(|entry| entry["module"].as_str().expect("module name"))
        .collect::<Vec<_>>();
    assert_eq!(module_names, vec!["demo.Util", "demo.Main"]);
    assert_eq!(
        modules[0]["source_path"],
        lib_dir.join("Util.terl").to_string_lossy().to_string()
    );
    assert_eq!(
        modules[1]["source_path"],
        app_dir.join("Main.terl").to_string_lossy().to_string()
    );
}
