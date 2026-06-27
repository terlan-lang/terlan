use std::path::{Path, PathBuf};

use super::{
    is_allowed_script, normalize_recipe_line, script_from_command, script_invocations_from_text,
};

/// Verifies Make recipe prefix normalization.
///
/// Inputs:
/// - Make recipe lines with common command prefixes.
///
/// Output:
/// - Normalized shell-like command text.
///
/// Transformation:
/// - Removes `@` and `-` prefixes after indentation.
#[test]
fn normalize_recipe_line_removes_make_command_prefixes() {
    assert_eq!(
        normalize_recipe_line("\t@-python3 tools/check_policy.py"),
        "python3 tools/check_policy.py"
    );
}

/// Verifies script extraction from direct interpreter commands.
///
/// Inputs:
/// - Shell-like recipe commands.
///
/// Output:
/// - Extracted script paths for Python and shell invocations.
///
/// Transformation:
/// - Skips interpreter flags and ignores non-script commands.
#[test]
fn script_from_command_extracts_interpreter_script_operands() {
    assert_eq!(
        script_from_command("python3 -B tools/check_policy.py --strict"),
        Some(PathBuf::from("tools/check_policy.py"))
    );
    assert_eq!(
        script_from_command("bash scripts/check_release.sh"),
        Some(PathBuf::from("scripts/check_release.sh"))
    );
    assert_eq!(script_from_command("cargo test -p terlan"), None);
}

/// Verifies script invocation extraction from Makefile text.
///
/// Inputs:
/// - Fixture Makefile text containing script and non-script recipes.
///
/// Output:
/// - Script invocation records with source line numbers.
///
/// Transformation:
/// - Scans every line and extracts direct interpreter-script invocations.
#[test]
fn script_invocations_from_text_keeps_makefile_locations() {
    let makefile = Path::new("Makefile");
    let text = "\
check:\n\
\t$(CARGO) test\n\
\t$(PYTHON) tools/check_policy.py\n\
\t@bash scripts/check_release.sh\n";

    let invocations = script_invocations_from_text(makefile, text);

    assert_eq!(invocations.len(), 2);
    assert_eq!(invocations[0].line_no, 3);
    assert_eq!(
        invocations[0].script,
        PathBuf::from("tools/check_policy.py")
    );
    assert_eq!(invocations[1].line_no, 4);
    assert_eq!(
        invocations[1].script,
        PathBuf::from("scripts/check_release.sh")
    );
}

/// Verifies allowed script hierarchy classification.
///
/// Inputs:
/// - Representative allowed and disallowed script paths.
///
/// Output:
/// - Boolean hierarchy classification.
///
/// Transformation:
/// - Applies exact script allowances and `check_*` release-root rules.
#[test]
fn is_allowed_script_accepts_release_owned_policy_scripts() {
    assert!(is_allowed_script(Path::new("tools/check_policy.py")));
    assert!(is_allowed_script(Path::new("std/scripts/check_summary.py")));
    assert!(is_allowed_script(Path::new(
        "std/scripts/build_interfaces.py"
    )));
    assert!(!is_allowed_script(Path::new(
        "crates/terlan/scripts/check_policy.py"
    )));
    assert!(!is_allowed_script(Path::new("tools/run_behavior_test.py")));
}
