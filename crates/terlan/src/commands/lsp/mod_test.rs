use std::process::ExitCode;

use super::run;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

#[test]
fn lsp_run_accepts_help_without_starting_server() {
    assert_eq!(run(&args(&["--help"])), ExitCode::SUCCESS);
}

#[cfg(not(feature = "editor-lsp"))]
#[test]
fn lsp_run_reports_feature_required_for_stdio_when_editor_lsp_is_disabled() {
    assert_eq!(run(&args(&["--stdio"])), ExitCode::from(2));
}

#[test]
fn lsp_run_rejects_multiple_arguments() {
    assert_eq!(run(&args(&["--stdio", "--extra"])), ExitCode::from(2));
}

#[test]
fn lsp_run_rejects_unknown_single_argument() {
    assert_eq!(run(&args(&["--tcp"])), ExitCode::from(2));
}
