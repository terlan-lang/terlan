use super::*;

#[test]
fn parse_args_accepts_core_v0_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "core-v0".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::CoreV0);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the frozen A0 Erlang artifact
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A0Erlang` while preserving the command and source path.
#[test]
fn parse_args_accepts_a0_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A0Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.1 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.1-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A01Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_1_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.1-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A01Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.2 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.2-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A02Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_2_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.2-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A02Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.3 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.3-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A03Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_3_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.3-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A03Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.4 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.4-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A04Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_4_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.4-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A04Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.5 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.5-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A05Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_5_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.5-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A05Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.6 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.6-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A06Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_6_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.6-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A06Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.7 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.7-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A07Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_7_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.7-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A07Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.8 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.8-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A08Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_8_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.8-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A08Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.9 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.9-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A09Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_9_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.9-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A09Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.10 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.10-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A010Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_10_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.10-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A010Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.11 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.11-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A011Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_11_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.11-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A011Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.12 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.12-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A012Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_12_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.12-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A012Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.13 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.13-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A013Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_13_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.13-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A013Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.14 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.14-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A014Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_14_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.14-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A014Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.15 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.15-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A015Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_15_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.15-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A015Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.16 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.16-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A016Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_16_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.16-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A016Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.17 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.17-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A017Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_17_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.17-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A017Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.18 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.18-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A018Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_18_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.18-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A018Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.19 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.19-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A019Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_19_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.19-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A019Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.20 Erlang successor
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.20-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A020Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_20_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.20-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A020Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}

/// Verifies CLI argument parsing accepts the named A0.21 Erlang diagnostic
/// target profile.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--target-profile a0.21-erlang`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the command state carries
///   `TargetProfile::A021Erlang` while preserving the command and source
///   path.
#[test]
fn parse_args_accepts_a0_21_erlang_target_profile() {
    let (state, cmd) = parse_args(vec![
        "check".into(),
        "src/example.terl".into(),
        "--target-profile".into(),
        "a0.21-erlang".into(),
    ]);

    assert_eq!(state.target_profile, TargetProfile::A021Erlang);
    assert_eq!(cmd.verb.as_deref(), Some("check"));
    assert_eq!(cmd.args, vec!["src/example.terl".to_string()]);
}
