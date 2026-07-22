use super::*;

/// Verifies CLI argument parsing defaults to the compiler-owned VM profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing a normal command and no
///   `--target-profile`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Protects the 0.0.7 no-Vm-default contract at the top-level CLI
///   parser rather than only in command-local build/run/test parsers.
#[test]
fn parse_args_defaults_target_profile_to_vm() {
    let (state, cmd) = parse_args(vec!["build".into()]);

    assert_eq!(state.target_profile, TargetProfile::Vm);
    assert_eq!(cmd.verb.as_deref(), Some("build"));
    assert!(cmd.args.is_empty());
}

#[test]
fn parse_args_rejects_core_v0_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "core-v0".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::Vm);
    assert_eq!(cmd.verb, None);
    assert!(cmd.args.is_empty());
}

/// Verifies CLI argument parsing accepts the full VM target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile vm`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::Vm` while preserving the command and source path.
#[test]
fn parse_args_accepts_vm_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "vm".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::Vm);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts `terlan-vm` as a VM profile alias.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile terlan-vm`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Keeps the target-profile spelling aligned with the public build/run
///   target name without exposing removed OTP/Erlang profiles.
#[test]
fn parse_args_accepts_terlan_vm_target_profile_alias() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "terlan-vm".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::Vm);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the JavaScript shared target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile js.shared`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::JsShared` while preserving the command and source path.
#[test]
fn parse_args_accepts_js_shared_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "js.shared".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::JsShared);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing treats the shorthand JavaScript profile as
/// the shared JavaScript profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile js`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::JsShared` while preserving the command and source path.
#[test]
fn parse_args_accepts_js_target_profile_alias() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "js".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::JsShared);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the JavaScript browser target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile js.browser`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::JsBrowser` while preserving the command and source path.
#[test]
fn parse_args_accepts_js_browser_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "js.browser".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::JsBrowser);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the JavaScript worker target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile js.worker`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::JsWorker` while preserving the command and source path.
#[test]
fn parse_args_accepts_js_worker_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "js.worker".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::JsWorker);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing rejects public Vm target profiles.
///
/// Inputs:
/// - Synthetic CLI arguments containing removed Vm profile names.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses each removed profile and asserts argument parsing stops before
///   producing a command or carrying a migration profile forward.
#[test]
fn parse_args_rejects_public_erlang_target_profiles() {
    for profile in ["erlang", "a0-vm", "a0.21-vm"] {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.terl".into(),
            "--target-profile".into(),
            profile.into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::Vm);
        assert_eq!(cmd.verb, None);
        assert!(cmd.args.is_empty());
    }
}
