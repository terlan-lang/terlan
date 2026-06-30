use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::{render_failure, QualityResult};

/// Summary produced by the test-hierarchy check.
///
/// Inputs:
/// - `invocation_count`: number of script invocations found in public
///   Makefiles.
///
/// Output:
/// - Stable success metric rendered by the command-line wrapper.
///
/// Transformation:
/// - Keeps Makefile script-gate count separate from diagnostics so release
///   output stays compact on success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestHierarchySummary {
    pub invocation_count: usize,
}

/// Script command referenced by a public Makefile recipe.
///
/// Inputs:
/// - `makefile`: repository-relative Makefile path.
/// - `line_no`: one-based recipe line number.
/// - `script`: repository-relative script path.
///
/// Output:
/// - Immutable invocation record for validation and diagnostics.
///
/// Transformation:
/// - Stores only the command shape needed by the hierarchy checker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptInvocation {
    pub makefile: PathBuf,
    pub line_no: usize,
    pub script: PathBuf,
}

impl ScriptInvocation {
    /// Returns the stable diagnostic prefix for this invocation.
    ///
    /// Inputs:
    /// - Invocation source Makefile path and line number.
    ///
    /// Output:
    /// - `path:line` text used in failure messages.
    ///
    /// Transformation:
    /// - Joins repository-relative Makefile path and line number.
    pub fn diagnostic_prefix(&self) -> String {
        format!("{}:{}", self.makefile.display(), self.line_no)
    }
}

/// Runs the test-hierarchy script-gate check.
///
/// Inputs:
/// - `root`: repository root containing public Makefiles and scripts.
///
/// Output:
/// - Success summary when all script gates are release-owned policy,
///   generator, drift, or orchestration checks.
/// - Diagnostics for missing, crate-local, or behavioral script-only gates.
///
/// Transformation:
/// - Extracts direct interpreter-script invocations from public Makefiles.
/// - Validates that script gates live under allowed release-owned roots.
pub fn run_test_hierarchy(root: &Path) -> QualityResult<TestHierarchySummary> {
    let invocations = iter_script_invocations(root)?;
    let mut diagnostics = Vec::new();
    for invocation in &invocations {
        diagnostics.extend(check_invocation(root, invocation));
    }

    if !diagnostics.is_empty() {
        return Err(render_failure("test-hierarchy", &diagnostics));
    }

    Ok(TestHierarchySummary {
        invocation_count: invocations.len(),
    })
}

/// Returns public Makefiles scanned by the hierarchy check.
///
/// Inputs:
/// - `root`: repository root.
///
/// Output:
/// - Absolute paths to Makefiles whose script gates are public.
///
/// Transformation:
/// - Resolves the fixed Makefile set used by root and module-level checks.
fn makefiles(root: &Path) -> [PathBuf; 3] {
    [
        root.join("Makefile"),
        root.join("crates/terlan/cli.mk"),
        root.join("std/stdlib.mk"),
    ]
}

/// Returns script invocations from public Makefiles.
///
/// Inputs:
/// - `root`: repository root containing the public Makefile set.
///
/// Output:
/// - Ordered script invocation records.
///
/// Transformation:
/// - Reads Makefiles line by line and extracts direct `bash`, `sh`, `python`,
///   and `python3` script invocations.
fn iter_script_invocations(root: &Path) -> QualityResult<Vec<ScriptInvocation>> {
    let mut invocations = Vec::new();
    for makefile in makefiles(root) {
        let relative = makefile
            .strip_prefix(root)
            .map_err(|err| format!("{}: failed to relativize path: {err}", makefile.display()))?
            .to_path_buf();
        let text = fs::read_to_string(&makefile)
            .map_err(|err| format!("{}: failed to read Makefile: {err}", makefile.display()))?;
        invocations.extend(script_invocations_from_text(&relative, &text));
    }
    Ok(invocations)
}

/// Returns script invocations from one Makefile text.
///
/// Inputs:
/// - `makefile`: repository-relative Makefile path.
/// - `text`: Makefile contents.
///
/// Output:
/// - Ordered script invocation records for direct interpreter calls.
///
/// Transformation:
/// - Normalizes recipe command prefixes and extracts script operands from
///   shell-like command lines.
pub(crate) fn script_invocations_from_text(makefile: &Path, text: &str) -> Vec<ScriptInvocation> {
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let command = normalize_recipe_line(line);
            script_from_command(&command).map(|script| ScriptInvocation {
                makefile: makefile.to_path_buf(),
                line_no: index + 1,
                script,
            })
        })
        .collect()
}

/// Returns a shell-like recipe line without Make command prefixes.
///
/// Inputs:
/// - Raw Makefile line.
///
/// Output:
/// - Stripped command text.
///
/// Transformation:
/// - Removes leading whitespace and common Make recipe prefixes such as `@`
///   and `-`.
pub(crate) fn normalize_recipe_line(line: &str) -> String {
    let mut command = line.trim();
    while command.starts_with('@') || command.starts_with('-') {
        command = command[1..].trim_start();
    }
    command.to_owned()
}

/// Extracts the first script operand from a direct interpreter command.
///
/// Inputs:
/// - Normalized Make recipe command.
///
/// Output:
/// - Repository-relative script path when the command directly invokes a `.py`
///   or `.sh` path through a known interpreter.
/// - `None` for non-script commands.
///
/// Transformation:
/// - Uses `shell-words` tokenization to avoid maintaining custom shell
///   splitting logic.
pub(crate) fn script_from_command(command: &str) -> Option<PathBuf> {
    let parts = shell_words::split(command).ok()?;
    let command_name = parts.first()?;
    if !matches!(
        command_name.as_str(),
        "bash" | "sh" | "python" | "python3" | "$(PYTHON)" | "$(PYTHON3)"
    ) {
        return None;
    }
    parts
        .iter()
        .skip(1)
        .filter(|part| !part.starts_with('-'))
        .map(PathBuf::from)
        .find(|path| {
            matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("py" | "sh")
            )
        })
}

/// Returns whether a script path fits the allowed hierarchy.
///
/// Inputs:
/// - Repository-relative script path.
///
/// Output:
/// - `true` for policy, drift, generator, and orchestration scripts.
/// - `false` for crate-local or feature-behavior scripts.
///
/// Transformation:
/// - Allows exact generator/orchestration scripts and `check_*` scripts under
///   release-owned script roots.
pub(crate) fn is_allowed_script(script: &Path) -> bool {
    let allowed_exact = [
        Path::new("tools/package_release_artifact.py"),
        Path::new("tools/validate_ebnf.py"),
        Path::new("std/scripts/build_interfaces.py"),
        Path::new("std/scripts/run_release_tests.sh"),
    ];
    if allowed_exact.iter().any(|allowed| script == *allowed) {
        return true;
    }
    let is_check_script = script
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("check_"));
    is_check_script
        && [
            Path::new("tools"),
            Path::new("scripts"),
            Path::new("std/scripts"),
        ]
        .iter()
        .any(|prefix| script.starts_with(prefix))
}

/// Validates one script invocation.
///
/// Inputs:
/// - `root`: repository root used to check script existence.
/// - `invocation`: script invocation discovered from a Makefile.
///
/// Output:
/// - Empty diagnostics when the invocation satisfies the hierarchy contract.
/// - Stable diagnostics for missing or disallowed script usage.
///
/// Transformation:
/// - Resolves the script path relative to the repository root and checks
///   existence plus hierarchy classification.
fn check_invocation(root: &Path, invocation: &ScriptInvocation) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let prefix = invocation.diagnostic_prefix();
    if !root.join(&invocation.script).exists() {
        diagnostics.push(format!(
            "{prefix}: script `{}` does not exist",
            invocation.script.display()
        ));
    }
    if invocation.script.starts_with("crates/terlan") {
        diagnostics.push(format!(
            "{prefix}: script `{}` is crate-local; move policy scripts to scripts/ or std/scripts/",
            invocation.script.display()
        ));
    }
    if !is_allowed_script(&invocation.script) {
        diagnostics.push(format!(
            "{prefix}: script `{}` is not an allowed policy, drift, generator, or orchestration gate",
            invocation.script.display()
        ));
    }
    diagnostics
}

#[cfg(test)]
#[path = "test_hierarchy_test.rs"]
mod test_hierarchy_test;
