use std::io::IsTerminal;

use crate::{ColorChoice, DiagnosticFormat};

/// Test-runner output color policy.
///
/// Inputs:
/// - A compiler diagnostic format selected by CLI flags.
///
/// Output:
/// - A small formatter for success and failure status labels.
///
/// Transformation:
/// - Converts global diagnostic color selection into test-runner ANSI styling
///   without changing JSON diagnostics or manifest output.
#[derive(Debug, Clone, Copy)]
pub(super) struct TestOutputStyle {
    color_enabled: bool,
}

impl TestOutputStyle {
    /// Builds a test-output style from the compiler diagnostic format.
    ///
    /// Inputs:
    /// - `format`: selected diagnostic output format.
    ///
    /// Output:
    /// - Style that colors text diagnostics when requested and leaves JSON
    ///   output uncolored.
    ///
    /// Transformation:
    /// - Honors `--color always|never|auto`; `auto` colors only when stdout is
    ///   attached to a terminal.
    pub(super) fn from_diagnostic_format(format: DiagnosticFormat) -> Self {
        let color = match format {
            DiagnosticFormat::Text { color } => color,
            DiagnosticFormat::Json => ColorChoice::Never,
        };
        Self {
            color_enabled: color_enabled(color),
        }
    }

    /// Styles a success status label.
    ///
    /// Inputs:
    /// - `text`: status text such as `ok`.
    ///
    /// Output:
    /// - Green ANSI-wrapped text when color is enabled; otherwise unchanged.
    ///
    /// Transformation:
    /// - Applies the shared Terlan test-runner success color convention.
    pub(super) fn success(self, text: &str) -> String {
        self.paint(text, "32")
    }

    /// Styles a failure status label.
    ///
    /// Inputs:
    /// - `text`: status text such as `FAILED`.
    ///
    /// Output:
    /// - Red ANSI-wrapped text when color is enabled; otherwise unchanged.
    ///
    /// Transformation:
    /// - Applies the shared Terlan test-runner failure color convention.
    pub(super) fn failure(self, text: &str) -> String {
        self.paint(text, "31")
    }

    /// Applies one ANSI color code when enabled.
    ///
    /// Inputs:
    /// - `text`: text to display.
    /// - `code`: ANSI SGR color code.
    ///
    /// Output:
    /// - Colored or plain display string.
    ///
    /// Transformation:
    /// - Keeps the caller free of escape-sequence details.
    fn paint(self, text: &str, code: &str) -> String {
        if self.color_enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}

/// Returns whether ANSI color should be emitted for one color choice.
///
/// Inputs:
/// - `choice`: CLI color policy.
///
/// Output:
/// - `true` when stdout should receive ANSI color codes.
///
/// Transformation:
/// - Converts explicit color flags and terminal auto-detection into a boolean
///   used by test result rendering.
fn color_enabled(choice: ColorChoice) -> bool {
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => std::io::stdout().is_terminal(),
    }
}

#[cfg(test)]
#[path = "style_test.rs"]
mod style_test;
