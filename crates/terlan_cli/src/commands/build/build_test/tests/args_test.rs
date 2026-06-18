use super::*;

/// Verifies `terlc build <path>` defaults to the Erlang target.
///
/// Inputs:
/// - A build argument vector containing only a source path.
///
/// Output:
/// - Test assertion only; parsed build arguments must contain the input path
///   and `BuildTarget::Erlang`.
///
/// Transformation:
/// - Converts string slices into CLI-like owned arguments, then runs the build
///   parser without executing a build.
#[test]
fn parse_build_args_defaults_to_erlang_target() {
    let parsed = parse_build_args(&args(&["src/main.terl"])).expect("build args should parse");

    assert_eq!(
        parsed,
        BuildArgs {
            path: "src/main.terl".to_string(),
            target: BuildTarget::Erlang,
            declarations: false,
        }
    );
}

/// Verifies bare `terlc build` defaults to the current directory.
///
/// Inputs:
/// - An empty build argument vector.
///
/// Output:
/// - Test assertion only; parsed build arguments must use `.` and
///   `BuildTarget::Erlang`.
///
/// Transformation:
/// - Runs the build parser with no command-specific arguments to lock the
///   default project build behavior.
#[test]
fn parse_build_args_defaults_to_current_directory() {
    let parsed = parse_build_args(&args(&[])).expect("empty build args should parse");

    assert_eq!(
        parsed,
        BuildArgs {
            path: ".".to_string(),
            target: BuildTarget::Erlang,
            declarations: false,
        }
    );
}

/// Verifies explicit Erlang target syntax is accepted.
///
/// Inputs:
/// - A build argument vector containing a source path and `--target erlang`.
///
/// Output:
/// - Test assertion only; parsed target must be `BuildTarget::Erlang` and the
///   source path must be preserved.
///
/// Transformation:
/// - Runs the build parser over explicit target syntax without invoking the
///   backend build pipeline.
#[test]
fn parse_build_args_accepts_explicit_erlang_target() {
    let parsed =
        parse_build_args(&args(&["src/main.terl", "--target", "erlang"])).expect("valid args");

    assert_eq!(parsed.target, BuildTarget::Erlang);
    assert_eq!(parsed.path, "src/main.terl");
}

/// Verifies explicit JavaScript target syntax is accepted.
///
/// Inputs:
/// - Build argument vectors containing source paths and JavaScript target
///   spellings.
///
/// Output:
/// - Test assertion only; parsed targets must carry normalized JS profiles.
///
/// Transformation:
/// - Runs argument parsing only, proving JS build target names are accepted
///   before filesystem or backend work starts.
#[test]
fn parse_build_args_accepts_js_targets() {
    let shared = parse_build_args(&args(&["src/main.terl", "--target", "js"])).expect("js target");
    let browser = parse_build_args(&args(&["src/main.terl", "--target", "js.browser"]))
        .expect("js browser target");
    let worker = parse_build_args(&args(&["src/main.terl", "--target", "js.worker"]))
        .expect("js worker target");

    assert_eq!(shared.target, BuildTarget::Js(TargetProfile::JsShared));
    assert_eq!(browser.target, BuildTarget::Js(TargetProfile::JsBrowser));
    assert_eq!(worker.target, BuildTarget::Js(TargetProfile::JsWorker));
}

/// Verifies build declarations are accepted as explicit command intent.
///
/// Inputs:
/// - A build argument vector containing a JS target and `--declarations`.
///
/// Output:
/// - Test assertion only; parsed build args must preserve declaration intent.
///
/// Transformation:
/// - Runs argument parsing without invoking the backend, proving declaration
///   emission can be requested before JS artifact work begins.
#[test]
fn parse_build_args_accepts_declarations_flag() {
    let parsed = parse_build_args(&args(&[
        "src/main.terl",
        "--target",
        "js",
        "--declarations",
    ]))
    .expect("build declarations args should parse");

    assert_eq!(parsed.target, BuildTarget::Js(TargetProfile::JsShared));
    assert!(parsed.declarations);
}

/// Verifies declaration output is rejected for the Erlang build target.
///
/// Inputs:
/// - A build command using `--declarations` without selecting a JS target.
///
/// Output:
/// - Test assertion only; command execution must return a usage-level failure.
///
/// Transformation:
/// - Runs the build command far enough to validate target-specific flag
///   ownership without reading source files or invoking `erlc`.
#[test]
fn build_command_rejects_declarations_for_erlang_target() {
    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: args(&["--declarations"]),
        },
        CliState::default(),
    );

    assert_eq!(status, ExitCode::from(2));
}

/// Verifies unsupported build targets return a stable parser diagnostic.
///
/// Inputs:
/// - A build argument vector containing a source path and unsupported
///   `--target wasm`.
///
/// Output:
/// - Test assertion only; parsing must fail with the unsupported-target text.
///
/// Transformation:
/// - Runs argument parsing only, proving unsupported targets are rejected before
///   filesystem or backend work starts.
#[test]
fn parse_build_args_rejects_unsupported_target() {
    let err =
        parse_build_args(&args(&["src/main.terl", "--target", "wasm"])).expect_err("bad target");

    assert!(err.contains("unsupported build target `wasm`"));
}
