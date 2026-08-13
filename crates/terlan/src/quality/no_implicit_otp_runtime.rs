use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Summary produced by the implicit OTP runtime audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoImplicitOtpRuntimeSummary {
    pub rule_count: usize,
    pub forbidden_fragment_count: usize,
}

/// One source marker required by the runtime-selection audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeSelectionRule {
    path: &'static str,
    marker: &'static str,
    reason: &'static str,
}

/// One public-surface fragment that must not reappear after the VM pivot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ForbiddenRuntimeFragment {
    path: &'static str,
    fragment: &'static str,
    reason: &'static str,
}

const RUNTIME_SELECTION_RULES: &[RuntimeSelectionRule] = &[
    RuntimeSelectionRule {
        path: "crates/terlan/src/main.rs",
        marker: "terlc build [file.terl|dir] [--target terlan-vm|js|wasm.core]",
        reason: "build usage must not expose the removed Erlang target",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/main.rs",
        marker: "target_profile: TargetProfile::Vm",
        reason: "top-level CLI default target profile must stay on the compiler-owned VM lane",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/main.rs",
        marker: "Global options: --diagnostic-format text|json --color auto|always|never --timings",
        reason: "global usage must not expose internal target-profile spellings",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/build/args.rs",
        marker: "let mut target = BuildTarget::TerlanVm",
        reason: "build command default target must stay on the compiler-owned VM lane",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/build/README.md",
        marker: "The 0.0.7 build command defaults to the compiler-owned Terlan VM artifact",
        reason:
            "build internals docs must describe the compiler-owned VM artifact path as the default",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/build/README.md",
        marker: "The VM default build path must not write `.erl` or `.beam`",
        reason: "build internals docs must not describe Erlang or BEAM output as default artifacts",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/build/project_manifest.README.md",
        marker: "bare `terlc build` selects the Terlan VM target",
        reason:
            "project manifest docs must separate legacy artifact metadata from the build default",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/build/project_manifest.README.md",
        marker: "`terlan-vm` is the default manifest artifact",
        reason: "project manifest docs must identify the compiler-owned VM artifact as the default",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/build/project_manifest.README.md",
        marker: "`beam-thin` is rejected at parse time.",
        reason: "project manifest docs must keep beam-thin scoped to rejected legacy metadata",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/build/mod.rs",
        marker: "terlc build artifact `beam-thin` was removed from the public build path",
        reason: "build command must reject legacy VM artifact contracts before emission",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/main.rs",
        marker: "terlc run [project-dir|file.terl|file.terls] [--target terlan-vm]",
        reason: "run usage must not expose the removed Erlang target",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/main.rs",
        marker: "terlc test [file.terl|dir] [--target terlan-vm|js|wasm]",
        reason: "test usage must not expose the removed Erlang target",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/main.rs",
        marker: "terlc repl [--help|-h] [--debug]",
        reason: "repl usage must not expose a removed runtime selector",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/run/mod.rs",
        marker: "let mut target = RunTarget::TerlanVm",
        reason: "run command default target must stay on the compiler-owned VM lane",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/run/mod.rs",
        marker: "build_command_for_run",
        reason: "run command must forward the implicit VM target to build explicitly",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/run/mod.rs",
        marker: "run target `erlang` was removed from the public CLI",
        reason: "run diagnostics must reject the removed Erlang target explicitly",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/test/mod.rs",
        marker: "unsupported test target",
        reason: "test diagnostics must reject unsupported runtime targets explicitly",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/test/mod.rs",
        marker: "let mut target = TestTarget::TerlanVm",
        reason: "test command default target must stay on the compiler-owned VM lane",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/test/mod.rs",
        marker: "test target `erlang` was removed from the public CLI",
        reason: "test diagnostics must reject the removed Erlang target explicitly",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/repl/mod.rs",
        marker: "repl_target_profile_for_runtime",
        reason: "repl runtime selection must own the effective target profile",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/repl/mod.rs",
        marker: "ReplRuntime::Vm => TargetProfile::Vm",
        reason: "VM REPL must compile through the compiler-owned VM profile",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/serve/mod.rs",
        marker: "dynamic handlers require an adjacent project root",
        reason: "serve must fail closed when VM handler metadata cannot be resolved",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/deploy/mod.rs",
        marker: "does not support legacy `beam-thin` artifacts",
        reason: "deploy planning must reject legacy VM artifact contracts",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/commands/deploy/mod.rs",
        marker: "does not support legacy [target.erlang.dependencies] metadata",
        reason: "deploy planning must reject legacy Erlang dependency contracts",
    },
    RuntimeSelectionRule {
        path: "std/http/README.md",
        marker: "Handler dispatch reports missing VM handler artifacts",
        reason:
            "std.http docs must describe VM handler diagnostics instead of VM handler execution",
    },
    RuntimeSelectionRule {
        path: "crates/terlan/src/main.rs",
        marker: "--handler-runtime static",
        reason: "serve usage must only expose the VM-neutral static handler runtime",
    },
];

const FORBIDDEN_RUNTIME_FRAGMENTS: &[ForbiddenRuntimeFragment] = &[
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/main.rs",
        fragment: "--target erlang",
        reason: "top-level public usage must not expose the removed Erlang target",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/main.rs",
        fragment: "--runtime beam",
        reason: "top-level public usage must not expose the removed VM runtime",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/main.rs",
        fragment: "otp-runtime",
        reason: "top-level public usage must not expose internal OTP runtime tooling",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/main.rs",
        fragment: "\"emit\" => commands::emit::run",
        reason: "top-level dispatcher must not restore the removed Erlang emit command",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/commands/mod.rs",
        fragment: "pub(crate) mod emit;",
        reason: "command registry must not restore the removed Erlang emit command",
    },
    ForbiddenRuntimeFragment {
        path: "std/http/README.md",
        fragment: "missing VM artifacts",
        reason:
            "std.http docs must not describe VM handler artifact lookup as the runtime contract",
    },
    ForbiddenRuntimeFragment {
        path: "std/http/README.md",
        fragment: "`erl`",
        reason: "std.http docs must not describe shelling out to erl for handler dispatch",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/commands/deploy/mod.rs",
        fragment: "runtime.beam",
        reason: "deploy planning must not produce a BEAM runtime capability",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/commands/build/mod.rs",
        fragment: "BuildTarget::Vm =>",
        reason: "build dispatcher must not retain a production Erlang target arm",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/commands/build/README.md",
        fragment: "starts `erl`",
        reason: "build docs must not describe the removed Erlang launcher as buildable",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/commands/build/README.md",
        fragment: "internal Erlang migration backend",
        reason: "build docs must not describe an active Erlang migration backend",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/commands/build/README.md",
        fragment: "src/<module>.erl",
        reason: "build docs must not document generated Erlang output layout",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/commands/build/README.md",
        fragment: "ebin/<module>.beam",
        reason: "build docs must not document generated BEAM output layout",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/commands/build/README.md",
        fragment: "Erlang target packaging adapter",
        reason: "build docs must not reopen the removed Erlang package adapter lane",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/commands/build/project_manifest.README.md",
        fragment: "external Vm/ERTS",
        reason: "manifest docs must not describe the removed Erlang launcher as buildable",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/validation/target_profile/README.md",
        fragment: "- `erlang`:",
        reason: "target-profile docs must not restore Erlang as a supported profile",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/validation/target_profile/README.md",
        fragment: "a0-erlang",
        reason: "target-profile docs must use VM A0 profile vocabulary",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/validation/target_profile/README.md",
        fragment: "for_erlang_profile",
        reason: "target-profile docs must not name removed Erlang-profile tests",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/commands/sql_runtime.rs",
        fragment: "terlan_sql_runtime.erl",
        reason: "SQL runtime docs must not describe generated Erlang helper consumers",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/commands/emit_native_metadata/artifacts.rs",
        fragment: "emit_native_boundary_erl_stub",
        reason: "NativeBoundary metadata emission must not restore generated Erlang loader stubs",
    },
    ForbiddenRuntimeFragment {
        path: "crates/terlan/src/commands/emit_native_metadata/artifacts.rs",
        fragment: "format!(\"{}.erl\"",
        reason: "NativeBoundary metadata emission must not write generated Erlang loader files",
    },
];

/// Runs the implicit OTP runtime audit.
///
/// Inputs:
/// - `root`: repository root.
///
/// Output:
/// - Success summary when all runtime-selection markers are present.
/// - Stable diagnostics when a command path loses explicit runtime/target
///   wording.
///
/// Transformation:
/// - Checks source-level command contracts so removed OTP/VM runtime
///   spellings cannot reappear in the public CLI surface.
pub fn run_no_implicit_otp_runtime(root: &Path) -> QualityResult<NoImplicitOtpRuntimeSummary> {
    let mut diagnostics = missing_runtime_selection_markers(root, RUNTIME_SELECTION_RULES)?;
    diagnostics.extend(forbidden_runtime_fragment_diagnostics(
        root,
        FORBIDDEN_RUNTIME_FRAGMENTS,
    )?);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(NoImplicitOtpRuntimeSummary {
        rule_count: RUNTIME_SELECTION_RULES.len(),
        forbidden_fragment_count: FORBIDDEN_RUNTIME_FRAGMENTS.len(),
    })
}

/// Returns diagnostics for missing runtime-selection markers.
fn missing_runtime_selection_markers(
    root: &Path,
    rules: &[RuntimeSelectionRule],
) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for rule in rules {
        let path = root.join(rule.path);
        let text = fs::read_to_string(&path)
            .map_err(|err| format!("cannot read runtime audit path {}: {err}", path.display()))?;
        if !text.contains(rule.marker) {
            diagnostics.push(format!(
                "`{}` is missing runtime marker `{}` ({})",
                rule.path, rule.marker, rule.reason
            ));
        }
    }
    Ok(diagnostics)
}

/// Returns diagnostics for forbidden public runtime fragments.
fn forbidden_runtime_fragment_diagnostics(
    root: &Path,
    fragments: &[ForbiddenRuntimeFragment],
) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for forbidden in fragments {
        let path = root.join(forbidden.path);
        let text = fs::read_to_string(&path)
            .map_err(|err| format!("cannot read runtime audit path {}: {err}", path.display()))?;
        if text.contains(forbidden.fragment) {
            diagnostics.push(format!(
                "`{}` exposes forbidden runtime fragment `{}` ({})",
                forbidden.path, forbidden.fragment, forbidden.reason
            ));
        }
    }
    Ok(diagnostics)
}

/// Renders audit diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[no-implicit-otp-runtime] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "no_implicit_otp_runtime_test.rs"]
#[cfg(test)]
mod no_implicit_otp_runtime_test;
