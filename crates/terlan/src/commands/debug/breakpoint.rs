use super::DebugCliError;

/// Validates one command-line breakpoint spec.
///
/// Inputs:
/// - `value`: raw argument following `--break`.
///
/// Output:
/// - `Ok(())` for `module.function`, `file:line`, or either shape followed
///   by `where <condition>`.
/// - `Err(DebugCliError)` for empty, malformed, or zero-line specs.
///
/// Transformation:
/// - Keeps the public debugger command contract precise before the VM debugger
///   runtime exists, so editor integrations can rely on stable diagnostics.
pub(super) fn validate_breakpoint_spec(value: &str) -> Result<(), DebugCliError> {
    if value.trim().is_empty() {
        return Err(invalid_breakpoint_spec(value));
    }

    let base = match split_conditional_breakpoint(value)? {
        BreakpointShape::Plain(base) => base,
        BreakpointShape::Conditional { base, condition } => {
            let _ = condition;
            base
        }
    };

    if let Some((path, line)) = base.rsplit_once(':') {
        return validate_file_line_breakpoint(value, path, line);
    }

    validate_module_function_breakpoint(value, base)
}

/// Parsed breakpoint condition boundary.
///
/// Inputs:
/// - Produced by splitting a raw breakpoint on a `where` guard marker.
///
/// Output:
/// - Plain base breakpoint or conditional breakpoint with non-empty condition.
///
/// Transformation:
/// - Keeps conditional breakpoint grammar explicit without parsing the future
///   debugger expression language at this layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BreakpointShape<'a> {
    Plain(&'a str),
    Conditional { base: &'a str, condition: &'a str },
}

/// Splits one optional conditional breakpoint suffix.
///
/// Inputs:
/// - `value`: raw breakpoint text.
///
/// Output:
/// - Plain breakpoint base or base plus condition.
///
/// Transformation:
/// - Recognizes ` where ` as the debugger guard boundary. Actual expression
///   parsing belongs to the future live debugger expression engine.
fn split_conditional_breakpoint(value: &str) -> Result<BreakpointShape<'_>, DebugCliError> {
    let Some((base, condition)) = value.split_once(" where ") else {
        return Ok(BreakpointShape::Plain(value));
    };

    let base = base.trim();
    let condition = condition.trim();
    if base.is_empty() || condition.is_empty() {
        return Err(DebugCliError {
            code: "debug_invalid_breakpoint_condition",
            message: format!("breakpoint `{value}` must include a breakpoint and condition"),
        });
    }

    Ok(BreakpointShape::Conditional { base, condition })
}

/// Validates a `file:line` breakpoint.
///
/// Inputs:
/// - `value`: original breakpoint text.
/// - `path`: text before the final colon.
/// - `line`: text after the final colon.
///
/// Output:
/// - `Ok(())` when the file path is non-empty and the line is positive.
/// - `Err(DebugCliError)` otherwise.
///
/// Transformation:
/// - Parses the line number as a stable CLI boundary check without touching the
///   file system; source existence belongs to the future live debugger.
fn validate_file_line_breakpoint(value: &str, path: &str, line: &str) -> Result<(), DebugCliError> {
    if path.trim().is_empty() {
        return Err(invalid_breakpoint_spec(value));
    }

    let Ok(line_number) = line.parse::<usize>() else {
        return validate_module_function_breakpoint(value, value);
    };
    if line_number == 0 {
        return Err(DebugCliError {
            code: "debug_invalid_breakpoint_line",
            message: format!("breakpoint `{value}` must use a positive line number"),
        });
    }

    Ok(())
}

/// Validates a `module.function` breakpoint.
///
/// Inputs:
/// - `value`: raw breakpoint text.
///
/// Output:
/// - `Ok(())` when every dot-separated segment is an identifier.
/// - `Err(DebugCliError)` otherwise.
///
/// Transformation:
/// - Treats the last segment as the function name and all preceding segments as
///   module path segments.
fn validate_module_function_breakpoint(value: &str, base: &str) -> Result<(), DebugCliError> {
    let parts: Vec<&str> = base.split('.').collect();
    if parts.len() < 2 || parts.iter().any(|part| !is_debug_identifier(part)) {
        return Err(invalid_breakpoint_spec(value));
    }

    Ok(())
}

/// Returns whether one debugger breakpoint segment is a Terlan-like identifier.
///
/// Inputs:
/// - `value`: one module or function segment.
///
/// Output:
/// - Boolean identifier classification.
///
/// Transformation:
/// - Allows ASCII letters, digits after the first character, and underscores.
fn is_debug_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Builds the stable malformed-breakpoint diagnostic.
///
/// Inputs:
/// - `value`: raw malformed breakpoint text.
///
/// Output:
/// - CLI error with stable code and message.
///
/// Transformation:
/// - Keeps all shape failures on one diagnostic code so integrations do not
///   need to classify parser internals.
pub(super) fn invalid_breakpoint_spec(value: &str) -> DebugCliError {
    DebugCliError {
        code: "debug_invalid_breakpoint",
        message: format!(
            "invalid breakpoint spec `{value}`; expected module.function or file:line"
        ),
    }
}
