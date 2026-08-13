use super::*;

/// Verifies direct VM file builds resolve sibling modules from their project.
///
/// Inputs:
/// - A manifest-backed package with an entry module and a sibling helper.
/// - A build command selecting only the entry source file.
///
/// Output:
/// - One linked project image containing the entry and imported sibling.
///
/// Transformation:
/// - Exercises package discovery and links the complete AOT application
///   closure required by `terlc run <file>`.
#[test]
fn build_command_resolves_project_sibling_for_direct_vm_file() {
    let dir = make_temp_dir("direct_vm_file_project_sibling");
    let project_dir = dir.join("project");
    let source_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("create project source dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n",
    )
    .expect("write project manifest");
    fs::write(
        source_dir.join("Helper.terl"),
        "module app.Helper.\n\npub answer(): Int ->\n    42.\n",
    )
    .expect("write sibling module");
    let entry = source_dir.join("Main.terl");
    fs::write(
        &entry,
        "module app.Main.\n\nimport app.Helper.{answer}.\n\npub main(): Int ->\n    answer().\n",
    )
    .expect("write entry module");

    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                entry.display().to_string(),
                "--target".to_string(),
                "terlan-vm".to_string(),
            ],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(out_dir.join(".terlan/app.Main.typi").is_file());
    let image = out_dir.join("vm/app_Main.tvm");
    assert_eq!(
        native_image_export_names(&image),
        vec!["app.Helper.answer/0", "app.Main.main/0"]
    );
    assert!(!out_dir.join("vm/app_Helper.tvm").exists());
    assert!(out_dir.join(".terlan/app.Helper.typi").is_file());
    let metadata = fs::read_to_string(out_dir.join(BUILD_PACKAGE_METADATA_FILE))
        .expect("direct project file build metadata");
    assert!(metadata.contains(r#""name": "app""#), "{metadata}");
}

/// Verifies VM project builds resolve local path dependency import closure.
///
/// Inputs:
/// - A root project manifest with a local `[dependencies]` path entry.
/// - A dependency project with its own manifest and source root.
/// - A root source file that imports a value from the dependency source.
///
/// Output:
/// - Test passes when one linked application image contains the dependency and
///   root native exports without writing legacy Erlang artifacts.
///
/// Transformation:
/// - Resolves the local path dependency manifest before VM artifact emission,
///   validates dependency source roots before the root source root, and feeds
///   dependency interfaces into root module typechecking.
#[test]
fn build_command_accepts_project_with_local_path_dependency_vm_import_closure() {
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
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n\n[dependencies]\nlocal_utils = { path = \"../local_utils\" }\n",
        )
        .expect("failed to write app manifest");
    fs::write(
            dep_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"local_utils\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n\n[native.rust]\ncrate = \"local_utils_native\"\npath = \"native\"\nhelper = \"local-utils-native-boundary\"\nhelper_env = \"LOCAL_UTILS_NATIVE_BOUNDARY_PATH\"\n",
        )
        .expect("failed to write dependency manifest");
    fs::write(
        dep_src.join("Util.terl"),
        "module local_utils.Util.\n\npub one(): Int ->\n    1.\n",
    )
    .expect("failed to write dependency module");
    fs::write(
            app_src.join("Main.terl"),
            "module app.Main.\n\nimport local_utils.Util.{one}.\n\npub main(): Int ->\n    value().\n\npub value(): Int ->\n    one().\n",
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
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(!out_dir.join("src").exists());
    assert!(!out_dir.join("ebin").exists());
    let image_path = out_dir.join("vm/app_Main.tvm");
    assert!(image_path.exists());
    assert!(!out_dir.join("vm/local_utils_Util.tvm").exists());
    assert_eq!(
        native_image_export_names(&image_path),
        vec![
            "app.Main.main/0",
            "app.Main.value/0",
            "local_utils.Util.one/0"
        ]
    );
}

fn native_image_export_names(path: &Path) -> Vec<String> {
    let image = fs::read(path).expect("read native application image");
    let target = crate::runtime::native_image::host_tvm_target().expect("host TVM target");
    let mut names = crate::runtime::native_image::inspect_tvm_image(&image, &target.triple)
        .expect("inspect native application image")
        .descriptor
        .exports
        .into_iter()
        .map(|export| export.name)
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Verifies package-native local dependencies reject unsupported targets.
///
/// Inputs:
/// - A consumer project with a local `terlan-polars` dependency declaring a
///   Rust native process helper.
/// - An explicit `js.shared` build target.
///
/// Output:
/// - Stable package/capability diagnostic and no backend artifacts.
///
/// Transformation:
/// - Resolves transitive manifest capability metadata before JS source
///   emission, proving native package failures identify the provider and
///   helper instead of falling through to a generic emitter diagnostic.
#[test]
fn build_command_rejects_polars_native_dependency_on_unsupported_target() {
    let dir = make_temp_dir("polars_native_dependency_unsupported_target");
    let app_dir = dir.join("app");
    let polars_dir = dir.join("terlan-polars");
    let app_src = app_dir.join("src/app");
    let polars_src = polars_dir.join("src/polars");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_src).expect("create app source");
    fs::create_dir_all(&polars_src).expect("create Polars source");
    fs::write(
        app_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n\n[dependencies]\nterlan-polars = { path = \"../terlan-polars\" }\n",
    )
    .expect("write app manifest");
    fs::write(
        polars_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"terlan-polars\"\nversion = \"0.1.0\"\nnamespace = \"polars\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n\n[native.rust]\ncrate = \"terlan_polars_native\"\npath = \"native\"\nhelper = \"terlan-polars-native-boundary\"\nhelper_env = \"TERLAN_NATIVE_BOUNDARY_HELPER_PATH\"\nfeatures = [\"real-polars\"]\n",
    )
    .expect("write Polars manifest");
    fs::write(
        app_src.join("Main.terl"),
        "module app.Main.\n\nimport polars.DataFrame.{height}.\n\npub rows(df: polars.DataFrame.DataFrame): Int ->\n    height(df).\n",
    )
    .expect("write app source");
    fs::write(
        polars_src.join("DataFrame.terl"),
        "module polars.DataFrame.\n\npub opaque type DataFrame.\n\n@compiler.native {polars.dataframe.height}\npub (_df: DataFrame) height(): Int ->\n    native.\n",
    )
    .expect("write Polars source");

    let error = validate_project_native_target(&app_dir, BuildTarget::Js(TargetProfile::JsShared))
        .expect_err("JS target should reject package native helper");
    assert_eq!(
        error,
        "error[package_native_target_unsupported]: target `js.shared` cannot build package `app` because local dependency `terlan-polars` requires native process helper `terlan-polars-native-boundary`; capability `native-process-helper` is currently supported only by target `terlan-vm`"
    );
    for (target, target_name) in [(BuildTarget::WasmCore, "wasm.core")] {
        let error = validate_project_native_target(&app_dir, target)
            .expect_err("non-VM target should reject package native helper");
        assert!(error.contains(&format!("target `{target_name}`")));
        assert!(error.contains("capability `native-process-helper`"));
    }

    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                app_dir.display().to_string(),
                "--target".to_string(),
                "js.shared".to_string(),
            ],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::from(1));
    assert!(!out_dir.exists());
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
            "terlan-vm".to_string(),
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
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::from(1));
    assert!(!out_dir.join("src/main.erl").exists());
    assert!(!out_dir.join("src/util.erl").exists());
    assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
}

/// Verifies legacy target dependency metadata is rejected before backend emission.
///
/// Inputs:
/// - A project manifest with a legacy `[target.erlang.dependencies]` section.
/// - A buildable source root.
///
/// Output:
/// - Test passes when build exits with failure and writes no artifacts.
///
/// Transformation:
/// - Rejects the legacy target-scoped dependency section and stops before
///   source-root emission.
#[test]
fn build_command_rejects_legacy_target_dependency_metadata_before_emission() {
    let dir = make_temp_dir("project_legacy_target_dependency_metadata");
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
            "terlan-vm".to_string(),
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
            "terlan-vm".to_string(),
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
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::from(1));
    assert!(!out_dir.join("src/main.erl").exists());
    assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
}
