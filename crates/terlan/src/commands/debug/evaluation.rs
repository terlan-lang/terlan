//! Pure selected-frame expression evaluation for the VM debugger.

use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

use super::DebugCliError;

#[cfg(test)]
#[path = "evaluation_test.rs"]
mod test;

/// Evaluates a side-effect-free expression with the normal VM compiler path.
///
/// Captured locals are bound to source literals before this entry point, so the
/// compiled expression is closed and cannot mutate the stopped frame.
pub(super) fn evaluate_frame_expression(expression: &str) -> Result<String, DebugCliError> {
    validate_closed_pure_expression(expression)?;
    let entry = format!("{}.", expression.trim().trim_end_matches('.'));
    let outputs = crate::commands::repl::evaluate_repl_prompt_inputs(
        &[entry],
        DiagnosticFormat::default(),
        NativePolicy::Pure,
        TargetProfile::Vm,
    )
    .map_err(|message| format!("error[vm.debugger.eval]: {message}"))?;
    Ok(outputs
        .last()
        .and_then(|lines| lines.last())
        .cloned()
        .ok_or_else(|| {
            "error[vm.debugger.eval_result]: pure evaluation produced no value".to_string()
        })?)
}

/// Replaces capture selectors with source-facing VM values before compilation.
pub(super) fn bind_frame_captures(
    expression: &str,
    captures: &[String],
) -> Result<String, DebugCliError> {
    let mut output = String::with_capacity(expression.len());
    let mut characters = expression.char_indices().peekable();
    let mut quote = None;
    let mut escaped = false;
    while let Some((_, character)) = characters.next() {
        if let Some(active_quote) = quote {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }
        if matches!(character, '"' | '\'') {
            quote = Some(character);
            output.push(character);
            continue;
        }
        if character != '$' {
            output.push(character);
            continue;
        }
        let mut digits = String::new();
        while let Some((_, next)) = characters.peek() {
            if !next.is_ascii_digit() {
                break;
            }
            digits.push(*next);
            characters.next();
        }
        if digits.is_empty() {
            return Err(
                "error[vm.debugger.local_selector]: capture selector must be `$<index>`"
                    .to_string()
                    .into(),
            );
        }
        let index = digits.parse::<usize>().map_err(|_| {
            "error[vm.debugger.local_selector]: capture selector is too large".to_string()
        })?;
        let value = captures.get(index).ok_or_else(|| {
            format!("error[vm.debugger.local_missing]: capture ${index} does not exist")
        })?;
        output.push('(');
        output.push_str(value);
        output.push(')');
    }
    Ok(output)
}

/// Replaces exact source-local identifiers outside quoted values.
pub(super) fn bind_frame_locals(
    expression: &str,
    names: &[String],
    captures: &[String],
) -> Result<String, DebugCliError> {
    let expression = bind_frame_captures(expression, captures)?;
    let mut output = String::with_capacity(expression.len());
    let mut characters = expression.chars().peekable();
    let mut quote = None;
    let mut escaped = false;
    while let Some(character) = characters.next() {
        if let Some(active_quote) = quote {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }
        if matches!(character, '"' | '\'') {
            quote = Some(character);
            output.push(character);
            continue;
        }
        if !is_identifier_start(character) {
            output.push(character);
            continue;
        }
        let mut identifier = String::from(character);
        while characters
            .peek()
            .is_some_and(|candidate| is_identifier_continue(*candidate))
        {
            identifier.push(characters.next().expect("peeked identifier character"));
        }
        if let Some(index) = names.iter().position(|name| name == &identifier) {
            let value = captures.get(index).ok_or_else(|| {
                format!(
                    "error[vm.debugger.local_metadata]: local `{identifier}` has no capture value"
                )
            })?;
            output.push('(');
            output.push_str(value);
            output.push(')');
        } else {
            output.push_str(&identifier);
        }
    }
    Ok(output)
}

fn is_identifier_start(character: char) -> bool {
    character == '_' || character.is_alphabetic()
}

fn is_identifier_continue(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}

fn validate_closed_pure_expression(expression: &str) -> Result<(), DebugCliError> {
    let expression = expression.trim();
    if expression.is_empty() {
        return Err(
            "error[vm.debugger.eval_empty]: evaluation requires an expression"
                .to_string()
                .into(),
        );
    }
    let mut identifier = String::new();
    let mut quoted = None;
    let mut escaped = false;
    for character in expression.chars().chain(std::iter::once(' ')) {
        if let Some(quote) = quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                quoted = None;
            }
            continue;
        }
        if matches!(character, '"' | '\'') {
            validate_identifier(&identifier)?;
            identifier.clear();
            quoted = Some(character);
            continue;
        }
        if character.is_alphanumeric() || character == '_' {
            identifier.push(character);
        } else {
            validate_identifier(&identifier)?;
            identifier.clear();
        }
    }
    if quoted.is_some() {
        return Err(
            "error[vm.debugger.eval_parse]: unterminated string in evaluation"
                .to_string()
                .into(),
        );
    }
    Ok(())
}

fn validate_identifier(identifier: &str) -> Result<(), DebugCliError> {
    if identifier.is_empty()
        || identifier
            .chars()
            .all(|character| character.is_ascii_digit())
        || matches!(identifier, "true" | "false" | "Unit")
    {
        return Ok(());
    }
    Err(format!(
        "error[vm.debugger.eval_side_effect]: `{identifier}` is not a closed literal; calls and frame mutation are rejected in pure debugger evaluation"
    )
    .into())
}
