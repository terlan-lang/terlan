use super::*;

/// Writes an executable helper script fixture.
///
/// Inputs:
/// - `path`: script path to create.
/// - `contents`: shell script contents.
///
/// Output:
/// - Executable file at `path`.
///
/// Transformation:
/// - Writes the script bytes and adds executable permission bits on Unix so
///   Erlang `open_port({spawn_executable, ...})` can run the helper.
fn write_executable_helper(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).expect("write helper fixture");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)
            .expect("read helper permissions")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("mark helper executable");
    }
}

/// Verifies portable collection constructor shorthand executes.
///
/// Inputs:
/// - A manifest-backed project importing `std.collections.List`.
/// - Source that constructs a populated list with `List(...)` instead of an
///   explicit `new[T]()` helper call and reads it with bracket syntax.
///
/// Output:
/// - Test passes when `terlc build` emits runnable BEAM artifacts and the
///   launcher prints the first list value.
///
/// Transformation:
/// - Exercises release summary loading, imported constructor vararg inference,
///   BEAM-native list constructor lowering, and zero-based list index lowering
///   through the generated executable.
#[test]
fn build_command_compiles_release_list_constructor_shorthand_and_index_read() {
    let dir = make_temp_dir("directory_project_collection_constructor_shorthand");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.collections.List.\n\
\n\
pub main(): Unit ->\n\
let users = List(\"Alice\", \"Bob\", \"Charlie\");\n\
println(users[0]).\n\
",
    )
    .expect("failed to write collection shorthand module");

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
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main");
    assert!(
        !erl_text.contains("std_collections_list:typer_ctor_list_varargs_0"),
        "list constructor should not call an unshipped std helper:\n{}",
        erl_text
    );
    assert!(
        erl_text.contains("lists:nth(0 + 1, Users)"),
        "list index read should lower through BEAM list indexing:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run list constructor launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "Alice\n");
}

/// Verifies native vector constructor shorthand executes through the bridge.
///
/// Inputs:
/// - A manifest-backed project importing `std.native.collections.Vector`.
/// - Source that constructs a populated vector with `Vector(...)` and reads it
///   with bracket syntax.
///
/// Output:
/// - Test passes when `terlc build` emits runnable BEAM artifacts and the
///   launcher prints the first vector value.
///
/// Transformation:
/// - Exercises release summary loading, imported native constructor vararg
///   inference, SafeNative bridge runtime emission, opaque handle allocation,
///   and vector index dispatch without lowering Vector to an ordinary BEAM
///   list in the user module.
#[test]
fn build_command_compiles_native_vector_constructor_shorthand_and_index_read() {
    let dir = make_temp_dir("directory_project_native_vector_constructor_shorthand");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.native.collections.Vector.\n\
\n\
pub main(): Unit ->\n\
let users = Vector(\"Alice\", \"Bob\", \"Charlie\");\n\
println(users[0]).\n\
",
    )
    .expect("failed to write native vector shorthand module");

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
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main");
    assert!(
        erl_text.contains("std_native_collections_vector_safe_native:from_list"),
        "vector constructor should lower through the native bridge:\n{}",
        erl_text
    );
    assert!(
        erl_text.contains("std_native_collections_vector_safe_native:get_at"),
        "vector index read should lower through the native bridge:\n{}",
        erl_text
    );
    assert!(
        !erl_text.contains("lists:nth"),
        "native vector index should not lower to BEAM list indexing:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run native vector constructor launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "Alice\n");
}

/// Verifies native vector runtime does not silently fall back on invalid helper
/// output.
///
/// Inputs:
/// - A manifest-backed project importing `std.native.collections.Vector`.
/// - Runtime environment pointing `TERLAN_NATIVE_VECTOR_RUNTIME_HELPER` at a
///   non-Terlan executable.
///
/// Output:
/// - Test passes when the launcher fails with a native-vector runtime protocol
///   diagnostic instead of using BEAM-owned fallback storage.
///
/// Transformation:
/// - Exercises generated BEAM runtime startup, helper protocol validation, and
///   the production rule that fallback requires an explicit opt-in variable.
#[test]
fn build_command_native_vector_rejects_invalid_runtime_helper_without_fallback() {
    let dir = make_temp_dir("directory_project_native_vector_bad_helper");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.native.collections.Vector.\n\
\n\
pub main(): Unit ->\n\
let users = Vector(\"Alice\", \"Bob\");\n\
println(users[0]).\n\
",
    )
    .expect("failed to write native vector bad helper module");

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
        .env("TERLAN_NATIVE_VECTOR_RUNTIME_HELPER", "/bin/echo")
        .env_remove("TERLAN_NATIVE_VECTOR_RUNTIME_ALLOW_BEAM_FALLBACK")
        .output()
        .expect("run native vector bad helper launcher");
    assert!(
        !launcher_output.status.success(),
        "launcher unexpectedly succeeded: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    let stderr = String::from_utf8_lossy(&launcher_output.stderr);
    assert!(
        stderr.contains("native_vector_runtime_protocol"),
        "expected native vector protocol error, got stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        stderr
    );
}

/// Verifies native vector runtime reports unavailable helper executables.
///
/// Inputs:
/// - A manifest-backed project importing `std.native.collections.Vector`.
/// - Runtime environment pointing `TERLAN_NATIVE_VECTOR_RUNTIME_HELPER` at a
///   nonexistent absolute path.
///
/// Output:
/// - Test passes when the launcher fails with the unavailable-helper diagnostic
///   instead of silently switching to fallback storage.
///
/// Transformation:
/// - Exercises the generated BEAM runtime branch where the helper cannot be
///   opened at all.
#[test]
fn build_command_native_vector_rejects_missing_runtime_helper_without_fallback() {
    let dir = make_temp_dir("directory_project_native_vector_missing_helper");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.native.collections.Vector.\n\
\n\
pub main(): Unit ->\n\
let users = Vector(\"Alice\", \"Bob\");\n\
println(users[0]).\n\
",
    )
    .expect("failed to write native vector missing helper module");

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
        .env(
            "TERLAN_NATIVE_VECTOR_RUNTIME_HELPER",
            dir.join("missing-terlc-helper"),
        )
        .env_remove("TERLAN_NATIVE_VECTOR_RUNTIME_ALLOW_BEAM_FALLBACK")
        .output()
        .expect("run native vector missing helper launcher");
    assert!(
        !launcher_output.status.success(),
        "launcher unexpectedly succeeded: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    let stderr = String::from_utf8_lossy(&launcher_output.stderr);
    assert!(
        stderr.contains("native_vector_runtime_unavailable"),
        "expected native vector unavailable error, got stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        stderr
    );
}

/// Verifies native vector runtime rejects malformed helper handle integers.
///
/// Inputs:
/// - A helper script that returns `ok_handle nope 1` for the constructor call.
/// - A manifest-backed project importing `std.native.collections.Vector`.
///
/// Output:
/// - Test passes when the launcher fails with `native_vector_invalid_integer`.
///
/// Transformation:
/// - Exercises generated BEAM parsing of helper `ok_handle` fields without
///   allowing malformed helper output to crash through `binary_to_integer`.
#[test]
fn build_command_native_vector_rejects_malformed_helper_handle_integer() {
    let dir = make_temp_dir("directory_project_native_vector_bad_handle_integer");
    let helper = dir.join("fake-vector-helper");
    write_executable_helper(
        &helper,
        "#!/usr/bin/env sh\nwhile IFS= read -r _line; do printf 'ok_handle nope 1\\n'; done\n",
    );
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.native.collections.Vector.\n\
\n\
pub main(): Unit ->\n\
let users = Vector(\"Alice\");\n\
println(users[0]).\n\
",
    )
    .expect("failed to write native vector bad handle integer module");

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
    let launcher_output = Command::new(out_dir.join("bin/app"))
        .env("TERLAN_NATIVE_VECTOR_RUNTIME_HELPER", helper)
        .env_remove("TERLAN_NATIVE_VECTOR_RUNTIME_ALLOW_BEAM_FALLBACK")
        .output()
        .expect("run native vector bad handle integer launcher");
    assert!(
        !launcher_output.status.success(),
        "launcher unexpectedly succeeded: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    let stderr = String::from_utf8_lossy(&launcher_output.stderr);
    assert!(
        stderr.contains("native_vector_invalid_integer"),
        "expected invalid integer error, got stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        stderr
    );
}

/// Verifies native vector runtime rejects non-term helper payloads.
///
/// Inputs:
/// - A helper script that returns a valid handle, then `ok_term aGVsbG8=`.
/// - A manifest-backed project importing `std.native.collections.Vector`.
///
/// Output:
/// - Test passes when the launcher fails with `native_vector_invalid_term`.
///
/// Transformation:
/// - Exercises generated BEAM payload decoding so Base64-valid but
///   non-external-term data cannot crash the runtime.
#[test]
fn build_command_native_vector_rejects_helper_payload_that_is_not_erlang_term() {
    let dir = make_temp_dir("directory_project_native_vector_bad_term_payload");
    let helper = dir.join("fake-vector-helper");
    write_executable_helper(
        &helper,
        "#!/usr/bin/env sh\ncount=0\nwhile IFS= read -r _line; do\n  count=$((count + 1))\n  if [ \"$count\" -eq 1 ]; then printf 'ok_handle 1 1\\n'; else printf 'ok_term aGVsbG8=\\n'; fi\ndone\n",
    );
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.native.collections.Vector.\n\
\n\
pub main(): Unit ->\n\
let users = Vector(\"Alice\");\n\
println(users[0]).\n\
",
    )
    .expect("failed to write native vector bad term payload module");

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
    let launcher_output = Command::new(out_dir.join("bin/app"))
        .env("TERLAN_NATIVE_VECTOR_RUNTIME_HELPER", helper)
        .env_remove("TERLAN_NATIVE_VECTOR_RUNTIME_ALLOW_BEAM_FALLBACK")
        .output()
        .expect("run native vector bad term payload launcher");
    assert!(
        !launcher_output.status.success(),
        "launcher unexpectedly succeeded: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    let stderr = String::from_utf8_lossy(&launcher_output.stderr);
    assert!(
        stderr.contains("native_vector_invalid_term"),
        "expected invalid term error, got stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        stderr
    );
}

/// Verifies native vector BEAM fallback is explicit and opt-in only.
///
/// Inputs:
/// - A manifest-backed project importing `std.native.collections.Vector`.
/// - Runtime environment pointing at an invalid helper while setting
///   `TERLAN_NATIVE_VECTOR_RUNTIME_ALLOW_BEAM_FALLBACK=1`.
///
/// Output:
/// - Test passes when the launcher succeeds through the compatibility fallback.
///
/// Transformation:
/// - Proves fallback behavior remains available for controlled tests while the
///   neighboring negative test proves it is not the production default.
#[test]
fn build_command_native_vector_allows_explicit_test_fallback() {
    let dir = make_temp_dir("directory_project_native_vector_explicit_fallback");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.native.collections.Vector.\n\
\n\
pub main(): Unit ->\n\
let users = Vector(\"Alice\", \"Bob\");\n\
users.push(\"Carol\");\n\
println(users[2]).\n\
",
    )
    .expect("failed to write native vector explicit fallback module");

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
        .env("TERLAN_NATIVE_VECTOR_RUNTIME_HELPER", "/bin/echo")
        .env("TERLAN_NATIVE_VECTOR_RUNTIME_ALLOW_BEAM_FALLBACK", "1")
        .output()
        .expect("run native vector explicit fallback launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "Carol\n");
}

/// Verifies native vector mutable receiver calls execute through the bridge.
///
/// Inputs:
/// - A manifest-backed project importing `std.native.collections.Vector`.
/// - Source that pushes a value through command-style receiver syntax and then
///   reads the updated vector by index.
///
/// Output:
/// - Test passes when the launcher prints the pushed value.
///
/// Transformation:
/// - Exercises compiler-side BEAM lowering for native vector mutable receiver
///   calls, bridge-handle rebinding, and subsequent indexed reads from the
///   updated handle.
#[test]
fn build_command_compiles_native_vector_mutable_receiver_push() {
    let dir = make_temp_dir("directory_project_native_vector_push");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.native.collections.Vector.\n\
\n\
pub main(): Unit ->\n\
let users = Vector(\"Alice\");\n\
users.push(\"Bob\");\n\
println(users[1]).\n\
",
    )
    .expect("failed to write native vector push module");

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
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main");
    assert!(
        erl_text.contains("_TerlanMutReceiver0 = std_native_collections_vector_safe_native:push"),
        "native vector push should lower through bridge rebinding:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run native vector push launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "Bob\n");
}

/// Verifies native vector handles remain isolated in generated BEAM code.
///
/// Inputs:
/// - A manifest-backed project that creates two vectors and mutates only the
///   first.
///
/// Output:
/// - Test passes when reads from each vector return their own storage values.
///
/// Transformation:
/// - Exercises source lowering, helper-backed handle allocation, mutable
///   receiver rebinding, and independent Rust-owned vector resources.
#[test]
fn build_command_compiles_native_vector_multiple_handle_isolation() {
    let dir = make_temp_dir("directory_project_native_vector_handle_isolation");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.native.collections.Vector.\n\
\n\
pub main(): Unit ->\n\
let left = Vector(\"Alice\");\n\
    right = Vector(\"Grace\");\n\
left.push(\"Bob\");\n\
println(left[1]);\n\
println(right[0]).\n\
",
    )
    .expect("failed to write native vector handle isolation module");

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
        .expect("run native vector handle isolation launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&launcher_output.stdout),
        "Bob\nGrace\n"
    );
}

/// Verifies native vector indexed assignment executes through the bridge.
///
/// Inputs:
/// - A manifest-backed project importing `std.native.collections.Vector`.
/// - Source that assigns through bracket syntax and reads the assigned slot.
///
/// Output:
/// - Test passes when the launcher prints the assigned value.
///
/// Transformation:
/// - Exercises parser support and compiler-side BEAM lowering for vector index
///   assignment through the `set_at` bridge call and command-style receiver
///   rebinding.
#[test]
fn build_command_compiles_native_vector_index_assignment() {
    let dir = make_temp_dir("directory_project_native_vector_index_assignment");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.native.collections.Vector.\n\
\n\
pub main(): Unit ->\n\
let users = Vector(\"Alice\", \"Bob\");\n\
users[1] = \"Carol\";\n\
println(users[1]).\n\
",
    )
    .expect("failed to write native vector index assignment module");

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
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main");
    assert!(
        erl_text.contains("_TerlanMutReceiver0 = std_native_collections_vector_safe_native:set_at"),
        "native vector indexed assignment should lower through bridge rebinding:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run native vector index assignment launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "Carol\n");
}

/// Verifies recursive binary search over a native vector.
///
/// Inputs:
/// - A manifest-backed project importing `std.native.collections.Vector` and
///   `std.core.Option`.
/// - Source that constructs a sorted vector, searches for a present value with
///   `/` midpoint arithmetic, and searches for a missing value.
///
/// Output:
/// - Test passes when the generated executable prints `found` then `missing`.
///
/// Transformation:
/// - Exercises vector constructor shorthand, vector receiver length, vector
///   indexed reads, recursive helper calls, integer `/` midpoint lowering,
///   `if` fallback clauses, and `Option` constructor-pattern matching through
///   BEAM lowering.
#[test]
fn build_command_compiles_native_vector_binary_search_algorithm() {
    let dir = make_temp_dir("directory_project_native_vector_binary_search");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.core.Option.{None, Some}.\n\
import type std.core.Option.Option.\n\
import std.native.collections.Vector.\n\
\n\
pub main(): Unit ->\n\
let users = Vector(1, 2, 3, 4, 5, 6, 7, 8, 9, 10);\n\
case binarySearch(users, 4, 0, users.len() - 1) {\n\
    Some(index) -> println(\"Element found at index: \" + Int.to_string(index));\n\
    None -> println(\"Element not found\")\n\
};\n\
case binarySearch(users, 42, 0, users.len() - 1) {\n\
    Some(_) -> println(\"found\");\n\
    None -> println(\"Element not found\")\n\
}.\n\
\n\
binarySearch(items: Vector[Int], target: Int, low: Int, high: Int): Option[Int] ->\n\
    if {\n\
        low > high -> None;\n\
        _ ->\n\
            let mid = low + ((high - low) / 2);\n\
                value = items[mid];\n\
            case value == target {\n\
                true -> Some(mid);\n\
                false ->\n\
                    if {\n\
                        value < target -> binarySearch(items, target, mid + 1, high);\n\
                        _ -> binarySearch(items, target, low, mid - 1)\n\
                    }\n\
            }\n\
    }.\n\
",
    )
    .expect("failed to write native vector binary search module");

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
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main");
    assert!(
        erl_text.contains("std_native_collections_vector_safe_native:from_list"),
        "binary search vector construction should use the native bridge:\n{}",
        erl_text
    );
    assert!(
        erl_text.contains("std_native_collections_vector_safe_native:get_at"),
        "binary search vector indexing should use the native bridge:\n{}",
        erl_text
    );
    assert!(
        erl_text.contains("div 2"),
        "integer midpoint division should lower to BEAM div:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run native vector binary search launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&launcher_output.stdout),
        "Element found at index: 3\nElement not found\n"
    );
}

/// Verifies generated Erlang specs for native vectors are bridge-qualified.
///
/// Inputs:
/// - A manifest-backed project with a function signature containing
///   `Vector[Int]` and `Option[Int]`.
/// - Defaulted integer parameters that keep the signature close to interactive
///   user examples while remaining compile-time constants.
///
/// Output:
/// - Test passes when `terlc build` succeeds, `erlc` accepts the generated
///   source, and the generated `-spec` uses the SafeNative vector bridge type.
///
/// Transformation:
/// - Protects against stale backend type emission such as `vector < _int >()`
///   by validating the complete build path rather than only the typechecker or
///   expression body lowering.
#[test]
fn build_command_emits_bridge_qualified_native_vector_specs() {
    let dir = make_temp_dir("directory_project_native_vector_spec_bridge");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.native.collections.Vector.\n\
import std.core.Option.{Option, Some, None}.\n\
\n\
pub main(): Unit ->\n\
let values = Vector(1, 2, 3, 4);\n\
case find(values, 3, 0, values.len() - 1) {\n\
    Some(index) -> println(\"found \" + index);\n\
    None -> println(\"missing\")\n\
}.\n\
\n\
find(values: Vector[Int], target: Int, low: Int = 0, high: Int = 100): Option[Int] ->\n\
    if {\n\
        low > high -> None;\n\
        _ ->\n\
            let mid = low + ((high - low) div 2);\n\
                value = values[mid];\n\
            case value == target {\n\
                true -> Some(mid);\n\
                false ->\n\
                    if {\n\
                        value < target -> find(values, target, mid + 1, high);\n\
                        _ -> find(values, target, low, mid - 1)\n\
                    }\n\
            }\n\
    }.\n\
",
    )
    .expect("failed to write native vector spec bridge module");

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
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main");
    assert!(
        erl_text.contains(
            "-spec find(std_native_collections_vector_safe_native:vector(integer()), integer(), integer(), integer()) -> std_core_option:typer_option(integer())."
        ),
        "native vector specs should reference the exported bridge type:\n{}",
        erl_text
    );
    assert!(
        !erl_text.contains("vector <") && !erl_text.contains("Vector <"),
        "native vector specs must not leak stale source type application spelling:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run native vector spec bridge launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&launcher_output.stdout),
        "found 2\n"
    );
}

/// Verifies generated Erlang specs for nested native vectors are valid.
///
/// Inputs:
/// - A manifest-backed project with a function returning
///   `Option[Vector[Int]]`.
/// - Source that pattern matches the returned option so nested constructor
///   specs are forced through Erlang generation.
///
/// Output:
/// - Test passes when `erlc` accepts the nested generated `-spec` and the
///   executable prints the success branch.
///
/// Transformation:
/// - Catches recursive type-spec lowering regressions where native vector
///   bridge mapping works for direct parameters but fails when nested inside
///   another type constructor.
#[test]
fn build_command_emits_bridge_qualified_nested_native_vector_specs() {
    let dir = make_temp_dir("directory_project_nested_native_vector_spec_bridge");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.native.collections.Vector.\n\
import std.core.Option.{Option, Some, None}.\n\
\n\
pub main(): Unit ->\n\
case maybe_values(true) {\n\
    Some(_) -> println(\"values\");\n\
    None -> println(\"missing\")\n\
}.\n\
\n\
maybe_values(enabled: Bool): Option[Vector[Int]] ->\n\
    if {\n\
        enabled -> Some(build_values());\n\
        _ -> None\n\
    }.\n\
\n\
build_values(): Vector[Int] ->\n\
    Vector(1, 2, 3).\n\
",
    )
    .expect("failed to write nested native vector spec bridge module");

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
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main");
    assert!(
        erl_text.contains(
            "-spec maybe_values(boolean()) -> std_core_option:typer_option(std_native_collections_vector_safe_native:vector(integer()))."
        ),
        "nested native vector specs should reference the exported bridge type:\n{}",
        erl_text
    );
    assert!(
        !erl_text.contains("vector <") && !erl_text.contains("Vector <"),
        "nested native vector specs must not leak stale source type application spelling:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run nested native vector spec bridge launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "values\n");
}

/// Verifies case-pattern bindings retain local function return payload types.
///
/// Inputs:
/// - A manifest-backed project with a local function returning
///   `Option[Vector[Int]]`.
/// - Source that matches `Some(values)` and calls `values.len()` in the branch.
///
/// Output:
/// - Test passes when the generated executable prints the native vector length.
///
/// Transformation:
/// - Catches regressions where local-call return types are not available to
///   case lowering, causing constructor-pattern payload bindings to lose their
///   receiver type and fall back to unsupported remote-call lowering.
#[test]
fn build_command_compiles_native_vector_receiver_from_option_pattern_binding() {
    let dir = make_temp_dir("directory_project_native_vector_option_pattern_receiver");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.native.collections.Vector.\n\
import std.core.Option.{Option, Some, None}.\n\
\n\
pub main(): Unit ->\n\
    case maybe_values(true) {\n\
        Some(values) -> println(Int.to_string(values.len()));\n\
        None -> println(\"missing\")\n\
    }.\n\
\n\
maybe_values(enabled: Bool): Option[Vector[Int]] ->\n\
    if {\n\
        enabled -> Some(Vector(1, 2, 3));\n\
        _ -> None\n\
    }.\n\
",
    )
    .expect("failed to write native vector option pattern receiver module");

    let state = CliState {
        out_dir,
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

    let status = run(cmd, state.clone());

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(state.out_dir.join("src/app_main.erl")).expect("read generated module");
    assert!(
        erl_text.contains("std_native_collections_vector_safe_native:length(Values)"),
        "pattern-bound native vector receiver should lower to bridge length call:\n{}",
        erl_text
    );

    let executable_path = state.out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run native vector option pattern receiver launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "3\n");
}

/// Verifies inline alias constructors can be matched as their union alias.
///
/// Inputs:
/// - A manifest-backed project importing `std.core.Option` and native vectors.
/// - Source that directly matches `Some(Vector(...))` and still includes a
///   `None` branch.
///
/// Output:
/// - Test passes when the generated executable prints the native vector length.
///
/// Transformation:
/// - Catches regressions where inline constructor scrutinees are inferred only
///   as their concrete runtime tuple and cannot be widened to a compatible
///   visible union alias such as `Option[T]`.
#[test]
fn build_command_compiles_inline_option_constructor_case_scrutinee() {
    let dir = make_temp_dir("directory_project_inline_option_constructor_case_scrutinee");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.native.collections.Vector.\n\
import std.core.Option.{Option, Some, None}.\n\
\n\
pub main(): Unit ->\n\
    case Some(Vector(1, 2, 3)) {\n\
        Some(values) -> println(Int.to_string(values.len()));\n\
        None -> println(\"missing\")\n\
    }.\n\
",
    )
    .expect("failed to write inline option constructor case module");

    let state = CliState {
        out_dir,
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

    let status = run(cmd, state.clone());

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(state.out_dir.join("src/app_main.erl")).expect("read generated module");
    assert!(
        erl_text.contains("std_native_collections_vector_safe_native:length(Values)"),
        "inline option case payload should keep native vector receiver type:\n{}",
        erl_text
    );

    let executable_path = state.out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run inline option constructor case launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "3\n");
}

/// Verifies inline `Result` constructors can be matched as their union alias.
///
/// Inputs:
/// - A manifest-backed project importing `std.core.Result` and native vectors.
/// - Source that directly matches `Ok(Vector(...))` while also including an
///   `Err` branch.
///
/// Output:
/// - Test passes when the generated executable prints the native vector length.
///
/// Transformation:
/// - Exercises constructor-scrutinee widening and syntax metadata propagation
///   through a two-parameter alias so `Result[T, E]` remains generic rather
///   than relying on Option-specific behavior.
#[test]
fn build_command_compiles_inline_result_constructor_case_scrutinee() {
    let dir = make_temp_dir("directory_project_inline_result_constructor_case_scrutinee");
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
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.native.collections.Vector.\n\
import std.core.Result.{Result, Ok, Err}.\n\
\n\
pub main(): Unit ->\n\
    case Ok(Vector(1, 2, 3)) {\n\
        Ok(values) -> println(Int.to_string(values.len()));\n\
        Err(code) -> println(Int.to_string(code))\n\
    }.\n\
",
    )
    .expect("failed to write inline result constructor case module");

    let state = CliState {
        out_dir,
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

    let status = run(cmd, state.clone());

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(state.out_dir.join("src/app_main.erl")).expect("read generated module");
    assert!(
        erl_text.contains("std_native_collections_vector_safe_native:length(Values)"),
        "inline result case payload should keep native vector receiver type:\n{}",
        erl_text
    );

    let executable_path = state.out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run inline result constructor case launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "3\n");
}
