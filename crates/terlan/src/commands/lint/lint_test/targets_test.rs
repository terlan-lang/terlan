use std::path::Path;

use super::lint_source;

/// Verifies incompatible target-specific std imports produce a stable warning.
#[test]
fn lint_reports_incompatible_target_std_imports() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.js.Promise.
import std.vm.Task.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:5:1"));
    assert!(rendered.contains("warning[TL0702:targets.incompatible-std]"));
    assert!(rendered.contains("source mixes incompatible target-specific std imports"));
}

/// Verifies VM and native imports can coexist because VM owns native calls.
#[test]
fn lint_accepts_vm_and_native_std_imports_together() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.native.collections.Vector.
import std.vm.Task.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0702"),
        "VM and native std imports must be compatible: {diagnostics:?}"
    );
}

/// Verifies target-looking imports inside comments are ignored.
#[test]
fn lint_accepts_target_std_imports_inside_comments() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

/**
 * import std.js.Promise.
 * import std.vm.Task.
 */
import std.vm.Task.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0702"),
        "commented target imports must not affect target compatibility: {diagnostics:?}"
    );
}
