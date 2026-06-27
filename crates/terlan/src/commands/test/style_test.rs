use super::TestOutputStyle;
use crate::{ColorChoice, DiagnosticFormat};

/// Verifies forced color wraps success labels in green.
///
/// Inputs:
/// - Text diagnostics with `ColorChoice::Always`.
///
/// Output:
/// - Assertion that the success formatter emits green ANSI text.
///
/// Transformation:
/// - Locks the test-runner color convention for passing tests.
#[test]
fn success_uses_green_when_color_is_forced() {
    let style = TestOutputStyle::from_diagnostic_format(DiagnosticFormat::Text {
        color: ColorChoice::Always,
    });

    assert_eq!(style.success("ok"), "\u{1b}[32mok\u{1b}[0m");
}

/// Verifies forced color wraps failure labels in red.
///
/// Inputs:
/// - Text diagnostics with `ColorChoice::Always`.
///
/// Output:
/// - Assertion that the failure formatter emits red ANSI text.
///
/// Transformation:
/// - Locks the test-runner color convention for failed tests.
#[test]
fn failure_uses_red_when_color_is_forced() {
    let style = TestOutputStyle::from_diagnostic_format(DiagnosticFormat::Text {
        color: ColorChoice::Always,
    });

    assert_eq!(style.failure("FAILED"), "\u{1b}[31mFAILED\u{1b}[0m");
}

/// Verifies disabled color leaves labels unchanged.
///
/// Inputs:
/// - Text diagnostics with `ColorChoice::Never`.
///
/// Output:
/// - Assertion that the formatter emits plain text.
///
/// Transformation:
/// - Ensures color can be disabled for logs, CI, and machine-captured output.
#[test]
fn color_never_leaves_labels_plain() {
    let style = TestOutputStyle::from_diagnostic_format(DiagnosticFormat::Text {
        color: ColorChoice::Never,
    });

    assert_eq!(style.success("ok"), "ok");
    assert_eq!(style.failure("FAILED"), "FAILED");
}
