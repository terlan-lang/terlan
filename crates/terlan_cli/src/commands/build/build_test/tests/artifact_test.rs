use super::*;

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
        "module app.Main.\n\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(\"Hello Terl\");\n    println(\"Hello Terl\").\n",
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
        "Hello Terl\nHello Terl\n"
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
