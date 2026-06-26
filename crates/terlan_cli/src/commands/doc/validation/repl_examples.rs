use super::{syntax_module_doc_blocks, DoctestError};
use terlan_syntax::SyntaxModuleOutput;

/// Execution mode for a REPL-backed documentation example.
///
/// Inputs:
/// - Parsed from `@example`, `@example ignore`, `@example error`, or
///   `@example target <name>` tags.
///
/// Output:
/// - Mode used by later doctest validation to run, skip, expect a diagnostic,
///   or run only for a named target profile.
///
/// Transformation:
/// - Keeps source documentation intent explicit without overloading prompt
///   syntax.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReplDocExampleMode {
    Run,
    Ignore,
    Error,
    Target(String),
}

/// One REPL input plus expected output lines from a documentation example.
///
/// Inputs:
/// - Parsed from prompt lines beginning with `>`.
///
/// Output:
/// - Prompt input and zero or more expected output lines.
///
/// Transformation:
/// - Separates REPL source from expected display text so validation can run
///   the prompt and compare output deterministically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplDocEntry {
    pub(crate) input: String,
    pub(crate) expected_output: Vec<String>,
}

/// Parsed REPL-backed documentation example.
///
/// Inputs:
/// - Produced by scanning syntax-output documentation blocks for `@example`.
///
/// Output:
/// - Example mode, entries, and approximate source span for diagnostics.
///
/// Transformation:
/// - Converts human-facing documentation into a typed prompt/expected-output
///   model consumed by later `doc --check` validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplDocExample {
    pub(crate) mode: ReplDocExampleMode,
    pub(crate) entries: Vec<ReplDocEntry>,
    pub(crate) offset: usize,
    pub(crate) len: usize,
}
/// Extracts REPL-backed examples from syntax-output documentation blocks.
///
/// Inputs:
/// - `module`: syntax-output module containing preserved documentation.
/// - `source`: original source text used for approximate example offsets.
///
/// Output:
/// - Ordered REPL documentation examples from module, declaration, field, and
///   trait-method docs.
///
/// Transformation:
/// - Scans `@example` tags, recognizes prompt lines beginning with `>`, and
///   attaches following non-tag output lines to the most recent prompt.
pub(crate) fn extract_repl_doc_examples(
    module: &SyntaxModuleOutput,
    source: &str,
) -> Vec<ReplDocExample> {
    let mut examples = Vec::new();
    for docs in syntax_module_doc_blocks(module) {
        examples.extend(repl_doc_examples_from_docs(docs, source));
    }
    examples
}

/// Validates REPL-backed documentation examples.
///
/// Inputs:
/// - `module`: syntax-output module containing preserved documentation.
/// - `source`: original source text used for diagnostic offsets.
/// - `diagnostic_format`: diagnostic mode used by compiler phases.
/// - `native_policy`: native-code policy enforced during compilation.
/// - `target_profile`: target-profile gate enforced during compilation.
///
/// Output:
/// - `Ok(())` when all runnable examples match their expected output, all
///   expected-error examples fail, and target-gated examples are either skipped
///   for non-matching targets or validated for matching targets.
/// - `Err(DoctestError)` for the first example extraction or validation
///   failure.
///
/// Transformation:
/// - Extracts `@example` prompt blocks, executes them through the non-
///   interactive REPL helper, skips ignored or non-matching target examples,
///   and compares line output.
pub(crate) fn validate_repl_doc_examples(
    module: &SyntaxModuleOutput,
    source: &str,
    diagnostic_format: crate::DiagnosticFormat,
    native_policy: crate::validation::native_policy::NativePolicy,
    target_profile: crate::validation::target_profile::TargetProfile,
) -> Result<(), DoctestError> {
    for example in extract_repl_doc_examples(module, source) {
        if example.mode == ReplDocExampleMode::Ignore {
            continue;
        }
        if let ReplDocExampleMode::Target(target) = &example.mode {
            if !repl_doc_example_target_matches(target, target_profile) {
                continue;
            }
        }
        let inputs = example
            .entries
            .iter()
            .map(|entry| entry.input.clone())
            .collect::<Vec<_>>();
        if example.mode == ReplDocExampleMode::Error {
            match crate::commands::repl::evaluate_repl_prompt_inputs(
                &inputs,
                diagnostic_format,
                native_policy,
                target_profile,
            ) {
                Ok(_) => {
                    return Err(DoctestError {
                        message: "expected REPL doc example to fail".to_string(),
                        offset: example.offset,
                        len: example.len,
                    })
                }
                Err(message) => {
                    let expected = example
                        .entries
                        .iter()
                        .flat_map(|entry| entry.expected_output.iter())
                        .collect::<Vec<_>>();
                    if expected.is_empty() || expected.iter().all(|line| message.contains(*line)) {
                        continue;
                    }
                    return Err(DoctestError {
                        message: format!(
                            "REPL doc error example mismatch: expected {:?}, got {:?}",
                            expected, message
                        ),
                        offset: example.offset,
                        len: example.len,
                    });
                }
            }
        }

        let actual = crate::commands::repl::evaluate_repl_prompt_inputs(
            &inputs,
            diagnostic_format,
            native_policy,
            target_profile,
        )
        .map_err(|message| DoctestError {
            message: format!("REPL doc example failed: {message}"),
            offset: example.offset,
            len: example.len,
        })?;
        for (entry, actual_output) in example.entries.iter().zip(actual) {
            if entry.expected_output != actual_output {
                return Err(DoctestError {
                    message: format!(
                        "REPL doc example output mismatch: expected {:?}, got {:?}",
                        entry.expected_output, actual_output
                    ),
                    offset: example.offset,
                    len: example.len,
                });
            }
        }
    }
    Ok(())
}

/// Returns whether a target-gated documentation example should run.
///
/// Inputs:
/// - `target`: normalized target label from `@example target <name>`.
/// - `target_profile`: active compiler target profile for `doc --check`.
///
/// Output:
/// - `true` when the target label applies to the active profile.
///
/// Transformation:
/// - Treats `erlang` as the family label for all Erlang release profiles and
///   otherwise requires an exact profile-name match.
fn repl_doc_example_target_matches(
    target: &str,
    target_profile: crate::validation::target_profile::TargetProfile,
) -> bool {
    let profile = target_profile.as_str();
    target == profile || (target == "erlang" && profile.ends_with("erlang"))
}
/// Extracts REPL examples from one documentation block.
///
/// Inputs:
/// - `docs`: documentation lines attached to one syntax item.
/// - `source`: original source text used for approximate example offsets.
///
/// Output:
/// - Ordered examples found in this doc block.
///
/// Transformation:
/// - Flattens lexer-preserved block docs into lines, then parses `@example`
///   sections until the next documentation tag or example tag.
fn repl_doc_examples_from_docs(docs: &[String], source: &str) -> Vec<ReplDocExample> {
    let mut examples = Vec::new();
    let mut active: Option<ReplDocExample> = None;
    let mut current_entry: Option<ReplDocEntry> = None;

    for line in docs.iter().flat_map(|doc| doc.lines()) {
        let trimmed = line.trim();
        if let Some(mode) = repl_doc_example_mode(trimmed) {
            finish_repl_doc_entry(&mut active, &mut current_entry);
            if let Some(example) = active.take() {
                examples.push(example);
            }
            let offset = source.find(trimmed).unwrap_or(0);
            active = Some(ReplDocExample {
                mode,
                entries: Vec::new(),
                offset,
                len: trimmed.len().max(1),
            });
            continue;
        }

        if active.is_none() {
            continue;
        }
        if trimmed.starts_with('@') {
            finish_repl_doc_entry(&mut active, &mut current_entry);
            if let Some(example) = active.take() {
                examples.push(example);
            }
            continue;
        }
        if trimmed.starts_with("```") || trimmed.is_empty() {
            continue;
        }
        if let Some(input) = trimmed.strip_prefix("> ") {
            finish_repl_doc_entry(&mut active, &mut current_entry);
            current_entry = Some(ReplDocEntry {
                input: input.trim().to_string(),
                expected_output: Vec::new(),
            });
            if let Some(example) = active.as_mut() {
                example.len = example.len.saturating_add(line.len() + 1);
            }
        } else if let Some(entry) = current_entry.as_mut() {
            entry.expected_output.push(trimmed.to_string());
            if let Some(example) = active.as_mut() {
                example.len = example.len.saturating_add(line.len() + 1);
            }
        }
    }

    finish_repl_doc_entry(&mut active, &mut current_entry);
    if let Some(example) = active {
        examples.push(example);
    }
    examples
}

/// Parses a documentation example tag into an execution mode.
///
/// Inputs:
/// - `line`: trimmed documentation line.
///
/// Output:
/// - Example mode for recognized `@example` lines.
/// - `None` for non-example documentation lines.
///
/// Transformation:
/// - Treats bare `@example` as runnable, `@example ignore` as skipped,
///   `@example error` as an expected diagnostic example, and
///   `@example target <name>` as runnable only for the matching target profile.
fn repl_doc_example_mode(line: &str) -> Option<ReplDocExampleMode> {
    let rest = line.strip_prefix("@example")?.trim();
    if rest.is_empty() {
        return Some(ReplDocExampleMode::Run);
    }
    if rest == "ignore" {
        return Some(ReplDocExampleMode::Ignore);
    }
    if rest == "error" {
        return Some(ReplDocExampleMode::Error);
    }
    if let Some(target) = rest.strip_prefix("target ") {
        let target = target.trim();
        if !target.is_empty()
            && target
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '.')
        {
            return Some(ReplDocExampleMode::Target(target.to_string()));
        }
    }
    None
}

/// Finalizes the current prompt entry into an active example.
///
/// Inputs:
/// - `active`: example currently being parsed.
/// - `current_entry`: prompt entry currently collecting output.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Moves the current entry into the active example when both exist.
fn finish_repl_doc_entry(
    active: &mut Option<ReplDocExample>,
    current_entry: &mut Option<ReplDocEntry>,
) {
    if let (Some(example), Some(entry)) = (active.as_mut(), current_entry.take()) {
        example.entries.push(entry);
    }
}
