use super::*;
use crate::commands::build::args::MobileBuildTarget;

/// Verifies `terlc build <path>` defaults to the Terlan VM target.
///
/// Inputs:
/// - A build argument vector containing only a source path.
///
/// Output:
/// - Test assertion only; parsed build arguments must contain the input path
///   and `BuildTarget::TerlanVm`.
///
/// Transformation:
/// - Converts string slices into CLI-like owned arguments, then runs the build
///   parser without executing a build.
#[test]
fn parse_build_args_defaults_to_terlan_vm_target() {
    let parsed = parse_build_args(&args(&["src/main.terl"])).expect("build args should parse");

    assert_eq!(
        parsed,
        BuildArgs {
            path: "src/main.terl".to_string(),
            target: BuildTarget::TerlanVm,
            target_explicit: false,
            declarations: false,
            native_codegen_policy: crate::compiler::native_ir::NativeCodegenPolicy::Development,
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
///   `BuildTarget::TerlanVm`.
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
            target: BuildTarget::TerlanVm,
            target_explicit: false,
            declarations: false,
            native_codegen_policy: crate::compiler::native_ir::NativeCodegenPolicy::Development,
        }
    );
}

/// Verifies release mode is explicit and development remains the default.
#[test]
fn parse_build_args_selects_explicit_native_release_policy() {
    let development = parse_build_args(&args(&["src/main.terl"])).expect("development build");
    let release = parse_build_args(&args(&["src/main.terl", "--release"])).expect("release build");

    assert_eq!(
        development.native_codegen_policy,
        crate::compiler::native_ir::NativeCodegenPolicy::Development
    );
    assert_eq!(
        release.native_codegen_policy,
        crate::compiler::native_ir::NativeCodegenPolicy::Release
    );
}

/// Verifies release policy cannot silently affect a non-native backend.
#[test]
fn build_command_rejects_release_policy_for_non_vm_target() {
    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: args(&["--target", "js", "--release"]),
        },
        CliState::default(),
    );

    assert_eq!(status, ExitCode::from(2));
}

/// Verifies explicit Vm target syntax is rejected.
///
/// Inputs:
/// - A build argument vector containing a source path and removed
///   `--target erlang`.
///
/// Output:
/// - Test assertion only; parser must report the removed public CLI target.
///
/// Transformation:
/// - Runs the build parser over explicit target syntax without invoking any
///   backend build pipeline.
#[test]
fn parse_build_args_rejects_explicit_erlang_target() {
    let err = parse_build_args(&args(&["src/main.terl", "--target", "erlang"]))
        .expect_err("erlang target should be removed");

    assert_eq!(
        err,
        "build target `erlang` was removed from the public CLI; use `terlan-vm`"
    );
}

/// Verifies build has no runtime or serialized-artifact fallback selector.
#[test]
fn parse_build_args_rejects_runtime_fallback_selection() {
    let error = parse_build_args(&args(&["src/main.terl", "--runtime", "interpreter"]))
        .expect_err("runtime fallback selector must be rejected");

    assert_eq!(error, "unknown build option: --runtime");
}

/// Rejects historical BEAM debug-info key options without reflecting secrets.
#[test]
fn key_compatibility_rejects_legacy_debug_info_key_options() {
    let expected = "debug-info key options were removed; Terlan VM artifacts use checksummed compiler metadata and never read ambient encryption keys";

    for build_args in [
        vec!["--debug-info-key", "secret-one"],
        vec!["--debug-info-key=secret-two"],
        vec!["--debug_info_key=secret-three"],
        vec!["+{debug_info_key,\"an old key\"}"],
    ] {
        let error = parse_build_args(&args(&build_args)).expect_err("debug key must be rejected");

        assert_eq!(error, expected);
        for secret in ["secret-one", "secret-two", "secret-three", "an old key"] {
            assert!(!error.contains(secret), "diagnostic leaked `{secret}`");
        }
    }
}

/// Rejects legacy compiler transforms without loading plugins or leaking input.
#[test]
fn compiler_transform_retirement_rejects_legacy_options_without_reflection() {
    let expected = "compiler transform options were removed; Terlan compiles checked source directly through CoreIR and VM IR";

    for build_args in [
        vec!["--parse-transform", "secret_parse_plugin"],
        vec!["--parse_transform=secret_parse_plugin"],
        vec!["--core-transform", "secret_core_plugin"],
        vec!["--core_transform=secret_core_plugin"],
        vec!["+{parse_transform,secret_parse_plugin}"],
        vec!["+{core_transform,secret_core_plugin}"],
    ] {
        let error =
            parse_build_args(&args(&build_args)).expect_err("compiler transform must be rejected");

        assert_eq!(error, expected);
        for secret in ["secret_parse_plugin", "secret_core_plugin"] {
            assert!(!error.contains(secret), "diagnostic leaked `{secret}`");
        }
    }
}

/// Keeps transform-like source path names outside the retired option policy.
#[test]
fn compiler_transform_retirement_accepts_transform_named_source_path() {
    let parsed = parse_build_args(&args(&["src/parse_transform_examples.terl"]))
        .expect("transform-like source path should remain valid");

    assert_eq!(parsed.path, "src/parse_transform_examples.terl");
    assert_eq!(parsed.target, BuildTarget::TerlanVm);
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
    assert!(shared.target_explicit);
    assert!(browser.target_explicit);
    assert!(worker.target_explicit);
}

/// Verifies build argument parsing accepts the Terlan VM artifact target.
///
/// Inputs:
/// - A build argument vector containing a source path and `--target terlan-vm`.
///
/// Output:
/// - Test assertion only; parsed target must be `BuildTarget::TerlanVm`.
///
/// Transformation:
/// - Runs argument parsing without invoking a backend so the post-OTP artifact
///   target is accepted before filesystem or compiler work starts.
#[test]
fn parse_build_args_accepts_terlan_vm_target() {
    let parsed = parse_build_args(&args(&["src/main.terl", "--target", "terlan-vm"]))
        .expect("terlan-vm target");

    assert_eq!(parsed.target, BuildTarget::TerlanVm);
    assert!(parsed.target_explicit);
}

/// Verifies build argument parsing accepts the first Wasm target.
///
/// Inputs:
/// - A build argument vector containing `--target wasm.core`.
///
/// Output:
/// - Test assertion only; parsed target must be `BuildTarget::WasmCore`.
///
/// Transformation:
/// - Keeps the promoted Wasm core target out of the reserved-family rejection
///   branch while browser/component targets remain reserved.
#[test]
fn parse_build_args_accepts_wasm_core_target() {
    let parsed = parse_build_args(&args(&["src/main.terl", "--target", "wasm.core"]))
        .expect("wasm.core target");

    assert_eq!(parsed.target, BuildTarget::WasmCore);
    assert!(parsed.target_explicit);
}

/// Verifies build argument parsing accepts the Android mobile planning target.
///
/// Inputs:
/// - A build argument vector containing a source path and
///   `--target mobile.android`.
///
/// Output:
/// - Test assertion only; parsed target must select Android mobile planning.
///
/// Transformation:
/// - Runs argument parsing without invoking native shell tooling so mobile
///   build planning can be accepted before package generation exists.
#[test]
fn parse_build_args_accepts_mobile_android_target() {
    let parsed = parse_build_args(&args(&["src/main.terl", "--target", "mobile.android"]))
        .expect("mobile.android target");

    assert_eq!(
        parsed.target,
        BuildTarget::Mobile(MobileBuildTarget::Android)
    );
    assert!(parsed.target_explicit);
}

/// Verifies build argument parsing accepts the iOS mobile planning target.
///
/// Inputs:
/// - A build argument vector containing a source path and
///   `--target mobile.ios`.
///
/// Output:
/// - Test assertion only; parsed target must select iOS mobile planning.
///
/// Transformation:
/// - Runs argument parsing without invoking Apple build tooling so mobile
///   build planning can be accepted before package generation exists.
#[test]
fn parse_build_args_accepts_mobile_ios_target() {
    let parsed = parse_build_args(&args(&["src/main.terl", "--target", "mobile.ios"]))
        .expect("mobile.ios target");

    assert_eq!(parsed.target, BuildTarget::Mobile(MobileBuildTarget::Ios));
    assert!(parsed.target_explicit);
}

/// Verifies future mobile build targets are reserved with stable diagnostics.
///
/// Inputs:
/// - Build argument vectors selecting mobile target spellings.
///
/// Output:
/// - Parse errors that identify the Mobile target family.
///
/// Transformation:
/// - Reserves the mobile target-profile names without claiming an implemented
///   mobile emitter.
#[test]
fn parse_build_args_rejects_reserved_mobile_targets() {
    for target in ["mobile", "mobile.shell"] {
        let error = parse_build_args(&args(&["src/main.terl", "--target", target]))
            .expect_err("mobile target should be reserved");

        assert!(
            error.contains("reserved for the Mobile target family"),
            "{target} should report Mobile family reservation: {error}"
        );
    }
}

/// Verifies constrained native targets fail before lowering with stable scope.
///
/// Inputs:
/// - Build argument vectors selecting each planned constrained native target.
///
/// Output:
/// - Parse errors identifying the target as reserved and unimplemented.
///
/// Transformation:
/// - Proves the compiler does not silently fall back to the host or VM target
///   while the feasibility contract has no artifact producer.
#[test]
fn parse_build_args_rejects_reserved_native_constrained_targets() {
    for target in [
        "native.no-std",
        "native.bare-metal",
        "native.kernel",
        "native.rtos",
        "native.riscv",
        "native.arm",
    ] {
        let error = parse_build_args(&args(&["src/main.terl", "--target", target]))
            .expect_err("native constrained target should be reserved");

        assert!(
            error.contains(
                "reserved for the native constrained target family but is not implemented yet"
            ),
            "{target} should report native constrained reservation: {error}"
        );
    }
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
    assert!(parsed.target_explicit);
    assert!(parsed.declarations);
}

/// Verifies declaration output is rejected for every non-JS build target.
///
/// Inputs:
/// - Build commands using `--declarations` with default VM, explicit VM,
///   Wasm, and mobile targets.
///
/// Output:
/// - Test assertion only; command execution must return a usage-level failure.
///
/// Transformation:
/// - Runs the build command far enough to validate target-specific flag
///   ownership without reading source files or invoking `erlc`.
#[test]
fn build_command_rejects_declarations_for_non_js_targets() {
    for build_args in [
        vec!["--declarations"],
        vec!["--declarations", "--target", "terlan-vm"],
        vec!["--declarations", "--target", "wasm.core"],
        vec!["--declarations", "--target", "mobile.android"],
        vec!["--declarations", "--target", "mobile.ios"],
    ] {
        let status = run(
            CliCommand {
                verb: Some("build".to_string()),
                args: args(&build_args),
            },
            CliState::default(),
        );

        assert_eq!(status, ExitCode::from(2), "build args: {build_args:?}");
    }
}

/// Verifies unsupported build targets return a stable parser diagnostic.
///
/// Inputs:
/// - A build argument vector containing a source path and unsupported
///   `--target python`.
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
        parse_build_args(&args(&["src/main.terl", "--target", "python"])).expect_err("bad target");

    assert!(err.contains("unsupported build target `python`"));
}

/// Verifies reserved Wasm build targets are classified before backend dispatch.
///
/// Inputs:
/// - Build argument vectors containing reserved Wasm target-family spellings.
///
/// Output:
/// - Test assertion only; parsing must fail with the reserved-family text.
///
/// Transformation:
/// - Runs argument parsing only, proving future Wasm targets cannot be routed
///   through the JavaScript backend while the Wasm implementation is absent.
#[test]
fn parse_build_args_target_family_rejects_reserved_wasm_targets() {
    for target in ["wasm", "wasm.browser", "wasm.component"] {
        let err =
            parse_build_args(&args(&["src/main.terl", "--target", target])).expect_err(target);

        assert!(
            err.contains(&format!(
                "build target `{target}` is reserved for the Wasm target family"
            )),
            "{target}: {err}"
        );
    }
}

/// Verifies reserved WASI build targets are classified before backend dispatch.
///
/// Inputs:
/// - Build argument vectors containing reserved WASI target-family spellings.
///
/// Output:
/// - Test assertion only; parsing must fail with the reserved-family text.
///
/// Transformation:
/// - Runs argument parsing only, proving future WASI targets cannot be routed
///   through the JavaScript backend while the WASI implementation is absent.
#[test]
fn parse_build_args_target_family_rejects_reserved_wasi_targets() {
    for target in ["wasi", "wasi.cli", "wasi.http", "wasi.worker"] {
        let err =
            parse_build_args(&args(&["src/main.terl", "--target", target])).expect_err(target);

        assert!(
            err.contains(&format!(
                "build target `{target}` is reserved for the WASI target family"
            )),
            "{target}: {err}"
        );
    }
}
