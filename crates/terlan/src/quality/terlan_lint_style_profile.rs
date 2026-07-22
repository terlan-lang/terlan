use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::roadmap_gate_integrity::parse_make_list_variable_values;
use crate::terlan_quality::{render_failure, QualityResult};

const PROFILE_PATH: &str = "docs/compiler/TERLAN_LINT_STYLE_PROFILE.md";
const ROADMAP_PATH: &str = "../docs/roadmap/ROADMAP_0_0_7.md";

const REQUIRED_FAMILIES: &[&str] = &[
    "Readability",
    "Imports",
    "Naming",
    "Docs",
    "Tests",
    "Std",
    "Effects",
    "Targets",
    "Interop",
    "Complexity",
    "Format Boundary",
];

const REQUIRED_SEVERITIES: &[&str] = &["error", "warning", "suggestion"];

const REQUIRED_COMMANDS: &[&str] = &[
    "terlc lint <file.terl|file.terli|dir>",
    "terlc lint --fix <file.terl|file.terli|dir>",
    "terlc fmt",
];

const REQUIRED_LINEAGE_MARKERS: &[&str] = &[
    "Google-style large-codebase style-guide principles",
    "clarity",
    "simplicity",
    "concision",
    "maintainability",
    "consistency",
    "Terlan syntax",
    "target inference",
    "VM ownership rules",
];

const REQUIRED_RULE_IDS: &[&str] = &[
    "TL0001", "TL0101", "TL0201", "TL0301", "TL0401", "TL0501", "TL0601", "TL0701", "TL0702",
    "TL0801", "TL0804", "TL0805", "TL0901", "TL0902", "TL0903", "TL1001", "TL1002", "TL1003",
];

const REQUIRED_PIPE_REJECTIONS: &[&str] = &[
    "named-argument ambiguity",
    "default-argument ambiguity",
    "function-value calls",
    "nested argument contexts",
    "side-effect-sensitive duplicated expressions",
    "target-specific intrinsic calls",
];

const REQUIRED_FORMAT_BOUNDARY_TERMS: &[&str] = &[
    "semicolon-separated expression chains",
    "pipe canonicalization belongs to lint",
    "fmt may split semicolon chains",
];
const REQUIRED_FIX_MARKERS: &[&str] = &["fix-safe", "fix-unsafe", "fix-unavailable"];

const PROHIBITED_PLACEHOLDERS: &[&str] = &["TODO", "TBD", "placeholder", "stub"];

const CHECK_PIPE_GATE: &str = "terlan-lint-pipe-canonicalization-check";
const CHECK_STDLIB_GATE: &str = "stdlib-check";

/// Summary produced by the Terlan lint style-profile gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerlanLintStyleProfileSummary {
    pub family_count: usize,
    pub rule_id_count: usize,
}

/// Runs the Terlan lint style-profile gate.
///
/// Inputs:
/// - `root`: golden repository root containing compiler docs and Makefile.
///
/// Output:
/// - Success when the lint profile names required rule families, commands,
///   severities, diagnostics, pipe-fix guardrails, and roadmap/Make gates.
/// - Stable diagnostics when the profile drifts or becomes non-executable.
///
/// Transformation:
/// - Treats lint style policy as a durable compiler contract for the
///   user-facing `terlc lint` command and future semantic lint rules.
pub fn run_terlan_lint_style_profile(root: &Path) -> QualityResult<TerlanLintStyleProfileSummary> {
    let profile = read_text(root, PROFILE_PATH)?;
    let makefile = read_text(root, "Makefile")?;
    let roadmap = read_text(root, ROADMAP_PATH)?;

    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_markers(
        PROFILE_PATH,
        &profile,
        "rule family",
        REQUIRED_FAMILIES,
    ));
    diagnostics.extend(validate_required_markers(
        PROFILE_PATH,
        &profile,
        "severity",
        REQUIRED_SEVERITIES,
    ));
    diagnostics.extend(validate_required_markers(
        PROFILE_PATH,
        &profile,
        "command",
        REQUIRED_COMMANDS,
    ));
    diagnostics.extend(validate_required_markers(
        PROFILE_PATH,
        &profile,
        "style lineage marker",
        REQUIRED_LINEAGE_MARKERS,
    ));
    diagnostics.extend(validate_required_markers(
        PROFILE_PATH,
        &profile,
        "rule id",
        REQUIRED_RULE_IDS,
    ));
    diagnostics.extend(validate_required_markers(
        PROFILE_PATH,
        &profile,
        "pipe rejection",
        REQUIRED_PIPE_REJECTIONS,
    ));
    diagnostics.extend(validate_required_markers(
        PROFILE_PATH,
        &profile,
        "format-boundary term",
        REQUIRED_FORMAT_BOUNDARY_TERMS,
    ));
    diagnostics.extend(validate_required_markers(
        PROFILE_PATH,
        &profile,
        "fix marker",
        REQUIRED_FIX_MARKERS,
    ));
    diagnostics.extend(validate_diagnostic_contract(&profile));
    diagnostics.extend(validate_rule_id_shapes(&profile));
    diagnostics.extend(validate_no_placeholder_terms(&profile));
    diagnostics.extend(validate_make_and_roadmap_hooks(&makefile, &roadmap));

    if diagnostics.is_empty() {
        Ok(TerlanLintStyleProfileSummary {
            family_count: REQUIRED_FAMILIES.len(),
            rule_id_count: REQUIRED_RULE_IDS.len(),
        })
    } else {
        Err(render_failure("terlan-lint-style-profile", &diagnostics))
    }
}

fn read_text(root: &Path, relative: &str) -> QualityResult<String> {
    let path = root.join(relative);
    fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read file: {err}", path.display()))
}

fn validate_required_markers(path: &str, text: &str, label: &str, markers: &[&str]) -> Vec<String> {
    markers
        .iter()
        .filter(|marker| !text.contains(**marker))
        .map(|marker| format!("{path}: missing {label} `{marker}`"))
        .collect()
}

fn validate_diagnostic_contract(profile: &str) -> Vec<String> {
    let required_terms = [
        "stable rule ID",
        "severity",
        "file and span",
        "short explanation",
        "fix availability marker",
    ];
    validate_required_markers(
        PROFILE_PATH,
        profile,
        "diagnostic contract term",
        &required_terms,
    )
}

fn validate_rule_id_shapes(profile: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut rule_ids = BTreeSet::new();
    for line in profile.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("- `TL") else {
            continue;
        };
        let Some((digits, _name)) = rest.split_once(' ') else {
            diagnostics.push(format!(
                "{PROFILE_PATH}: malformed rule ID line `{}`",
                trimmed
            ));
            continue;
        };
        if digits.len() != 4 || !digits.chars().all(|ch| ch.is_ascii_digit()) {
            diagnostics.push(format!(
                "{PROFILE_PATH}: rule ID must use TL plus four digits: `{}`",
                trimmed
            ));
        } else if !rule_ids.insert(digits.to_string()) {
            diagnostics.push(format!("{PROFILE_PATH}: duplicate rule ID `TL{digits}`"));
        }
    }
    diagnostics
}

fn validate_no_placeholder_terms(profile: &str) -> Vec<String> {
    PROHIBITED_PLACEHOLDERS
        .iter()
        .filter(|placeholder| profile.contains(**placeholder))
        .map(|placeholder| {
            format!(
                "{PROFILE_PATH}: lint profile must not contain placeholder term `{placeholder}`"
            )
        })
        .collect()
}

fn validate_make_and_roadmap_hooks(makefile: &str, roadmap: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for target in [
        "terlan-lint-style-profile-check",
        "terlan-lint-pipe-canonicalization-check",
    ] {
        if !has_make_target(makefile, target) {
            diagnostics.push(format!("Makefile: missing target `{target}`"));
        }
        if !roadmap.contains(target) {
            diagnostics.push(format!(
                "{ROADMAP_PATH}: missing roadmap reference to `{target}`"
            ));
        }
    }
    diagnostics.extend(validate_make_check_order(makefile));
    diagnostics
}

fn has_make_target(makefile: &str, target: &str) -> bool {
    let target_prefix = format!("{target}:");
    makefile
        .lines()
        .any(|line| line.trim_end().starts_with(&target_prefix))
}

fn validate_make_check_order(makefile: &str) -> Vec<String> {
    let check_gates = parse_make_list_variable_values(makefile, "CHECK_GATES");
    let Some(pipe_index) = check_gates.iter().position(|gate| gate == CHECK_PIPE_GATE) else {
        return vec![format!(
            "Makefile: `CHECK_GATES` must include `{CHECK_PIPE_GATE}`"
        )];
    };
    let Some(stdlib_index) = check_gates
        .iter()
        .position(|gate| gate == CHECK_STDLIB_GATE)
    else {
        return vec![format!(
            "Makefile: `CHECK_GATES` must include `{CHECK_STDLIB_GATE}`"
        )];
    };
    if pipe_index > stdlib_index {
        vec![format!(
            "Makefile: `CHECK_GATES` must run `{CHECK_PIPE_GATE}` before `{CHECK_STDLIB_GATE}`"
        )]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
#[path = "terlan_lint_style_profile_test.rs"]
mod terlan_lint_style_profile_test;
