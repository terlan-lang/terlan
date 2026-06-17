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
            target: BuildTarget::Erlang
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
            target: BuildTarget::Erlang
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

/// Verifies unsupported build targets return a stable parser diagnostic.
///
/// Inputs:
/// - A build argument vector containing a source path and unsupported
///   `--target js`.
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
        parse_build_args(&args(&["src/main.terl", "--target", "js"])).expect_err("bad target");

    assert!(err.contains("unsupported build target `js`"));
}
