use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// Verifies the lint style profile gate accepts a complete fixture.
///
/// Inputs:
/// - Temporary compiler doc and Makefile with required markers.
///
/// Output:
/// - Summary counts for required rule families and seed rule IDs.
///
/// Transformation:
/// - Keeps the style-profile gate contract independent from the repository
///   fixture while still validating the real checker.
#[test]
fn lint_style_profile_accepts_complete_fixture() {
    let root = temp_repo("lint_style_profile_accepts");
    write_fixture(&root, fixture_profile());

    let summary = run_terlan_lint_style_profile(&root).expect("complete fixture should pass");

    assert_eq!(summary.family_count, REQUIRED_FAMILIES.len());
    assert_eq!(summary.rule_id_count, REQUIRED_RULE_IDS.len());
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies missing rule families are rejected.
///
/// Inputs:
/// - Temporary profile missing one required family heading.
///
/// Output:
/// - Diagnostic naming the missing family.
///
/// Transformation:
/// - Prevents broad style categories from being silently dropped.
#[test]
fn lint_style_profile_rejects_missing_family() {
    let root = temp_repo("lint_style_profile_missing_family");
    write_fixture(
        &root,
        &fixture_profile().replace("Format Boundary", "Format Limits"),
    );

    let error = run_terlan_lint_style_profile(&root).expect_err("missing family should fail");

    assert!(error.contains("missing rule family `Format Boundary`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies the profile keeps the required Google-style lineage explicit.
///
/// Inputs:
/// - Temporary profile with the lineage sentence weakened to generic style
///   guidance.
///
/// Output:
/// - Diagnostic naming the missing lineage marker.
///
/// Transformation:
/// - Prevents the lint profile from drifting into ad hoc rule prose without
///   the agreed large-codebase style-guide basis.
#[test]
fn lint_style_profile_rejects_missing_google_style_lineage() {
    let root = temp_repo("lint_style_profile_missing_lineage");
    write_fixture(
        &root,
        &fixture_profile().replace(
            "Google-style large-codebase style-guide principles",
            "general style principles",
        ),
    );

    let error = run_terlan_lint_style_profile(&root).expect_err("missing lineage should fail");

    assert!(error.contains(
        "missing style lineage marker `Google-style large-codebase style-guide principles`"
    ));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies malformed rule IDs are rejected.
///
/// Inputs:
/// - Temporary profile with a rule ID missing one digit.
///
/// Output:
/// - Diagnostic explaining the rule ID format.
///
/// Transformation:
/// - Keeps diagnostics stable enough for suppressions, docs, and editor
///   quick-fixes.
#[test]
fn lint_style_profile_rejects_malformed_rule_id_shape() {
    let root = temp_repo("lint_style_profile_bad_rule_id");
    write_fixture(&root, &fixture_profile().replace("TL0001 ", "TL001 "));

    let error = run_terlan_lint_style_profile(&root).expect_err("bad rule ID should fail");

    assert!(error.contains("rule ID must use TL plus four digits"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies duplicate rule IDs are rejected.
///
/// Inputs:
/// - Temporary profile where two rule lines use the same stable ID.
///
/// Output:
/// - Diagnostic naming the duplicate rule ID.
///
/// Transformation:
/// - Prevents editor suppressions, docs, and generated diagnostics from
///   becoming ambiguous.
#[test]
fn lint_style_profile_rejects_duplicate_rule_id() {
    let root = temp_repo("lint_style_profile_duplicate_rule_id");
    write_fixture(
        &root,
        &fixture_profile().replace(
            "- `TL0001 readability.semicolon-chain`",
            "- `TL0001 readability.semicolon-chain`\n- `TL0001 readability.deep-expression`",
        ),
    );

    let error = run_terlan_lint_style_profile(&root).expect_err("duplicate rule ID should fail");

    assert!(error.contains("duplicate rule ID `TL0001`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies profile text cannot contain placeholders.
///
/// Inputs:
/// - Temporary profile with a TODO marker appended.
///
/// Output:
/// - Diagnostic naming the placeholder term.
///
/// Transformation:
/// - Keeps the lint profile as a release contract instead of an aspirational
///   note.
#[test]
fn lint_style_profile_rejects_placeholder_terms() {
    let root = temp_repo("lint_style_profile_placeholder");
    write_fixture(&root, &format!("{}\nTODO: add later.\n", fixture_profile()));

    let error = run_terlan_lint_style_profile(&root).expect_err("placeholder should fail");

    assert!(error.contains("placeholder term `TODO`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies format/lint boundary terms are required.
///
/// Inputs:
/// - Temporary profile missing the explicit pipe-canonicalization boundary.
///
/// Output:
/// - Diagnostic naming the missing boundary term.
///
/// Transformation:
/// - Keeps formatter behavior and semantic lint rewrites separated by an
///   executable style-profile contract.
#[test]
fn lint_style_profile_rejects_missing_format_boundary_term() {
    let root = temp_repo("lint_style_profile_missing_format_boundary");
    write_fixture(
        &root,
        &fixture_profile().replace(
            "pipe canonicalization belongs to lint",
            "pipe cleanup is lint",
        ),
    );

    let error =
        run_terlan_lint_style_profile(&root).expect_err("missing boundary term should fail");

    assert!(error.contains("missing format-boundary term `pipe canonicalization belongs to lint`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies concrete lint fix marker vocabulary is required.
///
/// Inputs:
/// - Temporary profile that keeps the generic fix availability phrase but
///   removes the concrete safe/unsafe/unavailable marker vocabulary.
///
/// Output:
/// - Diagnostic naming the missing fix marker.
///
/// Transformation:
/// - Keeps `terlc lint --fix` metadata explicit enough for CLI, editor, and
///   generated diagnostic consumers to distinguish safe rewrites from advice.
#[test]
fn lint_style_profile_rejects_missing_fix_marker_vocabulary() {
    let root = temp_repo("lint_style_profile_missing_fix_markers");
    write_fixture(
        &root,
        &fixture_profile().replace(
            "fix-safe fix-unsafe fix-unavailable",
            "fix availability marker",
        ),
    );

    let error = run_terlan_lint_style_profile(&root).expect_err("missing fix marker should fail");

    assert!(error.contains("missing fix marker `fix-safe`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies Make hooks are required.
///
/// Inputs:
/// - Complete profile with a Makefile missing one target.
///
/// Output:
/// - Diagnostic naming the missing executable target.
///
/// Transformation:
/// - Keeps lint requirements executable instead of prose-only.
#[test]
fn lint_style_profile_rejects_missing_make_target() {
    let root = temp_repo("lint_style_profile_missing_make");
    write_fixture(&root, fixture_profile());
    fs::write(
        root.join("Makefile"),
        [
            "CHECK_GATES := terlan-lint-pipe-canonicalization-check stdlib-check",
            "check: check-gates",
            "check-gates: $(CHECK_GATES)",
            "terlan-lint-style-profile-check:",
            "\tcargo test",
            "stdlib-check:",
            "\tcargo test",
            "",
        ]
        .join("\n"),
    )
    .expect("write Makefile");

    let error = run_terlan_lint_style_profile(&root).expect_err("missing target should fail");

    assert!(error.contains("Makefile: missing target `terlan-lint-pipe-canonicalization-check`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies the root check target keeps lint before stdlib gates.
///
/// Inputs:
/// - Complete profile with a Makefile whose `check` target runs stdlib before
///   the lint pipe gate.
///
/// Output:
/// - Diagnostic naming the required gate order.
///
/// Transformation:
/// - Prevents semantic lint regressions from being discovered only after
///   release-scale stdlib checks have already started.
#[test]
fn lint_style_profile_rejects_check_target_after_stdlib_order() {
    let root = temp_repo("lint_style_profile_check_order");
    write_fixture(&root, fixture_profile());
    fs::write(
        root.join("Makefile"),
        [
            "CHECK_GATES := stdlib-check terlan-lint-pipe-canonicalization-check",
            "check: check-gates",
            "check-gates: $(CHECK_GATES)",
            "terlan-lint-style-profile-check:",
            "\tcargo test",
            "terlan-lint-pipe-canonicalization-check:",
            "\tcargo test",
            "stdlib-check:",
            "\tcargo test",
            "",
        ]
        .join("\n"),
    )
    .expect("write Makefile");

    let error = run_terlan_lint_style_profile(&root).expect_err("bad order should fail");

    assert!(error.contains(
        "Makefile: `CHECK_GATES` must run `terlan-lint-pipe-canonicalization-check` before `stdlib-check`"
    ));
    fs::remove_dir_all(root).expect("remove fixture");
}

fn temp_repo(name: &str) -> PathBuf {
    let mut outer = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    outer.push(format!("terlan_quality_{name}_{nanos}"));
    let root = outer.join("terlan");
    fs::create_dir_all(root.join("docs/compiler")).expect("create compiler docs");
    root
}

fn write_fixture(root: &Path, profile: &str) {
    fs::write(root.join(PROFILE_PATH), profile).expect("write profile");
    fs::write(
        root.join("Makefile"),
        [
            "CHECK_GATES := terlan-lint-pipe-canonicalization-check stdlib-check",
            "check: check-gates",
            "check-gates: $(CHECK_GATES)",
            "terlan-lint-style-profile-check:",
            "\tcargo run -p terlan --bin terlan-quality --quiet -- terlan-lint-style-profile",
            "terlan-lint-pipe-canonicalization-check:",
            "\tcargo test",
            "stdlib-check:",
            "\tcargo test",
            "",
        ]
        .join("\n"),
    )
    .expect("write Makefile");
}

fn fixture_profile() -> &'static str {
    r#"# Terlan Lint Style Profile

`terlc lint <file.terl|file.terli|dir>`
`terlc lint --fix <file.terl|file.terli|dir>`
`terlc fmt`

Every lint diagnostic must have stable rule ID, severity, file and span, short explanation, and fix availability marker.
fix-safe fix-unsafe fix-unavailable

error warning suggestion

Google-style large-codebase style-guide principles
clarity simplicity concision maintainability consistency
Terlan syntax target inference VM ownership rules

Readability Imports Naming Docs Tests Std Effects Targets Interop Complexity Format Boundary

- `TL0001 readability.semicolon-chain`
- `TL0002 readability.deep-expression`
- `TL0003 readability.callback-name`
- `TL0004 readability.unused-destructure-binding`
- `TL0005 readability.redundant-comment`
- `TL0006 readability.public-docs`
- `TL0007 readability.doc-comment-spacing`
- `TL0008 readability.boolean-heavy-branch`
- `TL0009 readability.grouped-binding`
- `TL0010 readability.function-reference`
- `TL0101 imports.unused`
- `TL0201 naming.snake-case`
- `TL0301 docs.public-api`
- `TL0401 tests.fake`
- `TL0501 std.release-api-coverage`
- `TL0601 effects.hidden-ordering`
- `TL0701 targets.inference-ambiguous`
- `TL0702 targets.incompatible-std`
- `TL0801 interop.skip-manifest`
- `TL0804 interop.generated-source-manifest`
- `TL0805 interop.generated-lint-suppression`
- `TL0901 complexity.function-size`
- `TL0902 complexity.file-size`
- `TL0903 complexity.match-arm-size`
- `TL1001 format-boundary.semantic-fmt`
- `TL1002 format-boundary.pipe-fix`
- `TL1003 format-boundary.semicolon-split`

semicolon-separated expression chains
pipe canonicalization belongs to lint
fmt may split semicolon chains

named-argument ambiguity
default-argument ambiguity
function-value calls
nested argument contexts
side-effect-sensitive duplicated expressions
target-specific intrinsic calls
"#
}
