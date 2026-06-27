use super::*;

/// Verifies project builds can resolve selective imports from packaged std.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.io.Console.{println}` and calling the
///   imported function by its local name.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` succeeds and the
///   emitted Erlang calls the console runtime capability.
///
/// Transformation:
/// - Loads compiler-embedded std interface summaries for external project
///   compilation, resolves the selective import to its external target, and
///   lowers the target-neutral console call to Erlang `io:format`.
#[test]
fn build_command_resolves_selective_std_imports_from_external_project() {
    let dir = make_temp_dir("directory_project_selective_std_import");
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
        "module app.Main.\n\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(\"hello\").\n",
    )
    .expect("failed to write selective-import fixture");

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
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("io:format"),
        "selective std import should lower to Erlang console runtime call"
    );
    assert!(
        !erl_source.contains("println(\"hello\")"),
        "selective std import should not remain an unresolved local Erlang call"
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "hello\n");
}

/// Verifies selected std imports accept primitive receiver conversion calls.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.io.Console.{println}` and calling
///   `println(1.to_string())`.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a launcher
///   that prints `1`.
///
/// Transformation:
/// - Resolves the selected std import, lowers `Int.to_string` receiver syntax
///   through the compiler-owned primitive intrinsic path, and then lowers
///   `println` through the runtime console capability.
#[test]
fn build_command_compiles_selective_std_import_with_int_receiver_to_string() {
    let dir = make_temp_dir("directory_project_selective_std_import_int_to_string");
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
        "module app.Main.\n\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(1.to_string()).\n",
    )
    .expect("failed to write int receiver to_string fixture");

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
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "1\n");
}

/// Verifies selected primitive std function imports lower to intrinsics.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.io.Console.{println}` and
///   `std.core.Int.{to_string}`.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a launcher
///   that prints `2`.
///
/// Transformation:
/// - Resolves both selected std imports through compiler-embedded interfaces,
///   lowers `to_string(2)` through the compiler-owned primitive intrinsic
///   path, and lowers `println` through the runtime console capability.
#[test]
fn build_command_compiles_selective_std_import_with_int_to_string_function() {
    let dir = make_temp_dir("directory_project_selective_std_import_int_function_to_string");
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
        "module app.Main.\n\nimport std.io.Console.{println}.\nimport std.core.Int.{to_string}.\n\npub main(): Unit ->\n    println(to_string(2)).\n",
    )
    .expect("failed to write int function to_string fixture");

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
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("erlang:integer_to_list(2)"),
        "selected primitive import should lower to Erlang intrinsic: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "2\n");
}

/// Verifies imported primitive modules lower qualified calls to intrinsics.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.core.Int` as a module and calling
///   `Int.to_string(2)`.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a launcher
///   that prints `2`.
///
/// Transformation:
/// - Resolves the imported `Int` module alias to `std.core.Int`, recognizes
///   the method-shaped primitive module call, and lowers it through the
///   compiler-owned intrinsic path instead of emitting `int:to_string/1`.
#[test]
fn build_command_compiles_imported_int_module_to_string_call() {
    let dir = make_temp_dir("directory_project_imported_int_module_to_string_call");
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
        "module app.Main.\n\nimport std.io.Console.{println}.\nimport std.core.Int.\n\npub main(): Unit ->\n    println(Int.to_string(2)).\n",
    )
    .expect("failed to write imported int module fixture");

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
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("erlang:integer_to_list(2)"),
        "imported primitive module call should lower to Erlang intrinsic: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("int:to_string"),
        "imported primitive module call must not lower to a backend module call: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "2\n");
}

/// Verifies imported primitive Bool module calls lower to intrinsics.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.core.Bool` as a module and calling
///   `Bool.to_string(true)`.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a launcher
///   that prints `true`.
///
/// Transformation:
/// - Resolves the imported `Bool` module alias to `std.core.Bool`, recognizes
///   the primitive module call, and lowers it through the compiler-owned
///   intrinsic path instead of emitting `std_core_bool`.
#[test]
fn build_command_compiles_imported_bool_module_to_string_call() {
    let dir = make_temp_dir("directory_project_imported_bool_module_to_string_call");
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
        "module app.Main.\n\nimport std.io.Console.{println}.\nimport std.core.Bool.\n\npub main(): Unit ->\n    println(Bool.to_string(true)).\n",
    )
    .expect("failed to write imported bool module fixture");

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
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        !erl_source.contains("std_core_bool"),
        "imported primitive Bool module call must not lower to backend std module: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "true\n");
}

/// Verifies selected std imports are typechecked before backend emission.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.io.Console.{println}` and calling it with an
///   `Int` instead of the declared `String` argument.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` fails before
///   writing a user-facing launcher.
///
/// Transformation:
/// - Resolves the selected std import through compiler-embedded interface
///   summaries and proves argument mismatches are rejected by the formal
///   typecheck phase rather than leaking to Erlang runtime `badarg`.
#[test]
fn build_command_rejects_selective_std_import_argument_mismatch() {
    let dir = make_temp_dir("directory_project_selective_std_import_type_error");
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
        "module app.Main.\n\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(1).\n",
    )
    .expect("failed to write selective-import mismatch fixture");

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
    assert!(
        !out_dir.join("bin/app").exists(),
        "type errors should stop before launcher emission"
    );
    assert!(
        !out_dir.join("src/app_main.erl").exists(),
        "type errors should stop before Erlang source emission"
    );
}
