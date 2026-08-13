use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Summary produced by the OTP runtime exit inventory gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtpRuntimeExitSummary {
    pub required_term_count: usize,
    pub removal_lane_count: usize,
    pub closeout_blocker_count: usize,
}

const DOC_PATH: &str = "docs/runtime/OTP_RUNTIME_EXIT.md";

const REQUIRED_TERMS: &[&str] = &[
    "0.0.7 exit condition",
    "no active stock OTP runtime dependency",
    "terlan-vm",
    "std.vm",
    "migration bridge",
    "reference-only",
    "not compatibility gates",
    "remove the generated Erlang default path",
    "remove the `erlc` execution path",
    "remove the `erl` runtime invocation",
];

const REMOVAL_LANES: &[&str] = &[
    "`terlc run --target erlang`",
    "`terlc test --target erlang`",
    "`terlc repl --runtime beam`",
    "`terlc serve` dynamic handler execution",
    "`erlc` bridge",
    "`erl` runtime invocation",
];

const CLOSEOUT_BLOCKERS: &[&str] = &[];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "otp compatibility must be restored",
    "otp is the 0.0.7 runtime contract",
    "beam is the default terlan runtime",
    "`terlc serve` beam-backed handler lane",
    "beam-backed `terlc serve` handlers",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Runs the OTP runtime exit inventory gate.
///
/// Inputs:
/// - `root`: repository root.
///
/// Output:
/// - Success summary when the runtime exit document records all required
///   removal lanes.
/// - Stable diagnostics when the document loses the 0.0.7 no-OTP contract.
///
/// Transformation:
/// - Treats remaining OTP runtime paths as explicit removal lanes, not as
///   accepted compatibility surface.
pub fn run_otp_runtime_exit(root: &Path) -> QualityResult<OtpRuntimeExitSummary> {
    let path = root.join(DOC_PATH);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read OTP exit inventory: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_otp_runtime_exit_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(OtpRuntimeExitSummary {
        required_term_count: REQUIRED_TERMS.len(),
        removal_lane_count: REMOVAL_LANES.len(),
        closeout_blocker_count: CLOSEOUT_BLOCKERS.len(),
    })
}

/// Validates the OTP runtime exit inventory text.
fn validate_otp_runtime_exit_text(text: &str) -> Vec<String> {
    let normalized = normalize(text);
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&normalize(term)) {
            diagnostics.push(format!("missing OTP exit term `{term}`"));
        }
    }
    for lane in REMOVAL_LANES {
        if !text.contains(lane) {
            diagnostics.push(format!("missing OTP removal lane `{lane}`"));
        }
    }
    for blocker in CLOSEOUT_BLOCKERS {
        if !text.contains(blocker) {
            diagnostics.push(format!("missing OTP closeout blocker `{blocker}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(&normalize(claim)) {
            diagnostics.push(format!("forbidden OTP runtime claim `{claim}`"));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(&normalize(placeholder)) {
            diagnostics.push(format!(
                "placeholder OTP runtime exit text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

/// Normalizes prose for stable term checks.
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Renders gate diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[otp-runtime-exit] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "otp_runtime_exit_test.rs"]
#[cfg(test)]
mod otp_runtime_exit_test;
