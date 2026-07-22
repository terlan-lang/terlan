use std::path::Path;

use super::lint_source;

/// Verifies function clauses over the size threshold are reported.
#[test]
fn lint_reports_function_clause_size_threshold() {
    let source = long_function_source(41);

    let diagnostics = lint_source(Path::new("LongFunction.terl"), &source);

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("LongFunction.terl:6:5"));
    assert!(rendered.contains("warning[TL0901:complexity.function-size]"));
    assert!(rendered.contains("function clause exceeds the maintainability line threshold"));
}

/// Verifies function clauses at the size threshold remain accepted.
#[test]
fn lint_accepts_function_clause_size_threshold() {
    let source = long_function_source(34);

    let diagnostics = lint_source(Path::new("ShortFunction.terl"), &source);

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0901"),
        "threshold-sized function clause must be accepted: {diagnostics:?}"
    );
}

/// Verifies oversized case arms are reported separately from function size.
#[test]
fn lint_reports_match_arm_size_threshold() {
    let source = case_arm_source(21);

    let diagnostics = lint_source(Path::new("LongArm.terl"), &source);

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("LongArm.terl:"));
    assert!(rendered.contains("warning[TL0903:complexity.match-arm-size]"));
    assert!(rendered.contains("case or if arm exceeds the maintainability line threshold"));
}

/// Verifies branch arms at the threshold remain accepted.
#[test]
fn lint_accepts_match_arm_size_threshold() {
    let source = case_arm_source(20);

    let diagnostics = lint_source(Path::new("ShortArm.terl"), &source);

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0903"),
        "threshold-sized match arm must be accepted: {diagnostics:?}"
    );
}

/// Verifies ordinary source over the file-size threshold is reported.
#[test]
fn lint_reports_ordinary_source_file_size_threshold() {
    let source = oversized_source("module sample.\n", "// filler\n", 501);

    let diagnostics = lint_source(Path::new("Large.terl"), &source);

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Large.terl:1:1"));
    assert!(rendered.contains("warning[TL0902:complexity.file-size]"));
    assert!(rendered.contains("ordinary source file exceeds the maintainability line threshold"));
}

/// Verifies ordinary source at the threshold remains accepted.
#[test]
fn lint_accepts_ordinary_source_file_at_size_threshold() {
    let source = oversized_source("module sample.\n", "// filler\n", 499);

    let diagnostics = lint_source(Path::new("AtThreshold.terl"), &source);

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0902"),
        "threshold-sized source must be accepted: {diagnostics:?}"
    );
}

/// Verifies generated source is exempt from ordinary source size linting.
#[test]
fn lint_accepts_generated_source_file_size_with_manifest_policy() {
    let source = oversized_source(
        "/**\n * @generated true\n * source-manifest: typescript:lib.dom.d.ts#Huge\n */\nmodule generated.Huge.\n",
        "// generated filler\n",
        501,
    );

    let diagnostics = lint_source(Path::new("Generated.terl"), &source);

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0902"),
        "generated source size is governed by generated-file policy: {diagnostics:?}"
    );
}

fn oversized_source(prefix: &str, filler_line: &str, filler_count: usize) -> String {
    let mut source = String::from(prefix);
    for _ in 0..filler_count {
        source.push_str(filler_line);
    }
    source
}

fn long_function_source(step_count: usize) -> String {
    let mut source = String::from(
        "module sample.\n\n\
         /**\n\
         * Runs a generated-size fixture function.\n\
         */\n\
         pub run(): Unit ->\n",
    );
    for index in 0..step_count {
        source.push_str(&format!("    step_{index}();\n"));
    }
    source.push_str("    done().\n");
    source
}

fn case_arm_source(body_line_count: usize) -> String {
    let mut source = String::from(
        "module sample.\n\n\
         pub run(flag: Bool): Unit ->\n\
             case flag {\n\
                 true ->\n",
    );
    for index in 0..body_line_count.saturating_sub(1) {
        source.push_str(&format!("            step_{index}();\n"));
    }
    source.push_str(
        "            done();\n\
                 false -> done()\n\
             }.\n",
    );
    source
}
