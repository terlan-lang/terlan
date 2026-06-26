use super::repl_examples::{
    extract_repl_doc_examples, validate_repl_doc_examples, ReplDocExampleMode,
};
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;

/// Verifies `@example` tags are extracted from preserved block docs.
///
/// Inputs:
/// - One parsed module containing runnable, ignored, error, and target
///   examples.
///
/// Output:
/// - Four typed REPL examples with modes, prompt input, and expected
///   output.
///
/// Transformation:
/// - Parses source through the formal syntax-output path, then extracts
///   REPL prompt examples from block documentation.
#[test]
fn extracts_repl_doc_examples_from_block_docs() {
    let source = r#"module doc_examples.

/**
 * Adds one.
 *
 * @example
 * > 1 + 2.
 * 3
 *
 * @example ignore
 * > expensive().
 * skipped
 *
 * @example error
 * > missing.
 * error[resolve_error]: unknown name
 *
 * @example target rust
 * > native_only().
 */
pub add(x: Int): Int ->
    x + 1.
"#;
    let module = terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

    let examples = extract_repl_doc_examples(&module, source);

    assert_eq!(examples.len(), 4);
    assert_eq!(examples[0].mode, ReplDocExampleMode::Run);
    assert_eq!(examples[0].entries[0].input, "1 + 2.");
    assert_eq!(examples[0].entries[0].expected_output, vec!["3"]);
    assert_eq!(examples[1].mode, ReplDocExampleMode::Ignore);
    assert_eq!(examples[1].entries[0].input, "expensive().");
    assert_eq!(examples[2].mode, ReplDocExampleMode::Error);
    assert_eq!(examples[2].entries[0].input, "missing.");
    assert_eq!(
        examples[2].entries[0].expected_output,
        vec!["error[resolve_error]: unknown name"]
    );
    assert_eq!(
        examples[3].mode,
        ReplDocExampleMode::Target("rust".to_string())
    );
    assert_eq!(examples[3].entries[0].input, "native_only().");
}

/// Verifies fenced prompt examples stop before later doc tags.
///
/// Inputs:
/// - One parsed module containing an `@example` block wrapped in a text
///   fence followed by `@param`.
///
/// Output:
/// - One example whose expected output excludes fence delimiters and the
///   later parameter documentation.
///
/// Transformation:
/// - Confirms the extractor accepts docs-friendly fenced prompt examples
///   while preserving tag boundaries for later documentation metadata.
#[test]
fn extracts_fenced_repl_doc_example_until_next_tag() {
    let source = r#"module doc_examples.

/**
 * Adds one.
 *
 * @example
 * ```text
 * > 2 + 2.
 * 4
 * ```
 *
 * @param x The value.
 */
pub add(x: Int): Int ->
    x + 1.
"#;
    let module = terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

    let examples = extract_repl_doc_examples(&module, source);

    assert_eq!(examples.len(), 1);
    assert_eq!(examples[0].mode, ReplDocExampleMode::Run);
    assert_eq!(examples[0].entries.len(), 1);
    assert_eq!(examples[0].entries[0].input, "2 + 2.");
    assert_eq!(examples[0].entries[0].expected_output, vec!["4"]);
}

/// Verifies runnable REPL doc examples execute through REPL semantics.
///
/// Inputs:
/// - One parsed module with an example that imports `println` and calls it.
///
/// Output:
/// - Successful validation when expected output includes import `Unit`,
///   printed console output, and final expression `Unit`.
///
/// Transformation:
/// - Extracts prompt entries and runs them through the non-interactive REPL
///   validator used by `terlc doc --check`.
#[test]
fn validates_runnable_repl_doc_example_output() {
    let source = r#"module doc_examples.

/**
 * Prints a greeting.
 *
 * @example
 * > import std.io.Console.{println}.
 * Unit
 * > println("hello").
 * hello
 * Unit
 */
pub greet(): Unit ->
    Unit.
"#;
    let module = terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

    validate_repl_doc_examples(
        &module,
        source,
        crate::DiagnosticFormat::default(),
        NativePolicy::SafeNativeOptional,
        TargetProfile::Erlang,
    )
    .expect("validate examples");
}

/// Verifies runnable REPL doc examples fail on output mismatch.
///
/// Inputs:
/// - One parsed module whose example expects the wrong arithmetic result.
///
/// Output:
/// - `DoctestError` describing the output mismatch.
///
/// Transformation:
/// - Runs the prompt through the same validation path as successful
///   examples, then compares actual and expected output lines.
#[test]
fn rejects_runnable_repl_doc_example_output_mismatch() {
    let source = r#"module doc_examples.

/**
 * Adds numbers.
 *
 * @example
 * > 1 + 2.
 * 4
 */
pub add(x: Int): Int ->
    x + 1.
"#;
    let module = terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

    let error = validate_repl_doc_examples(
        &module,
        source,
        crate::DiagnosticFormat::default(),
        NativePolicy::SafeNativeOptional,
        TargetProfile::Erlang,
    )
    .expect_err("reject mismatch");

    assert!(error.message.contains("output mismatch"));
}

/// Verifies expected-error examples match returned diagnostic text.
///
/// Inputs:
/// - One parsed module with an `@example error` block expecting
///   `resolve_error`.
///
/// Output:
/// - Successful validation when the REPL prompt fails with matching
///   diagnostic content.
///
/// Transformation:
/// - Executes the prompt through REPL-backed validation and compares the
///   returned compiler diagnostic text against the expected example output.
#[test]
fn validates_expected_error_repl_doc_example() {
    let source = r#"module doc_examples.

/**
 * Demonstrates an unresolved name.
 *
 * @example error
 * > missing_value.
 * unknown REPL variable
 */
pub value(): Int ->
    1.
"#;
    let module = terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

    validate_repl_doc_examples(
        &module,
        source,
        crate::DiagnosticFormat::default(),
        NativePolicy::SafeNativeOptional,
        TargetProfile::Erlang,
    )
    .expect("validate expected error");
}

/// Verifies target-gated examples are skipped for non-matching targets.
///
/// Inputs:
/// - One parsed module with a Rust-only prompt that would fail under the
///   current REPL if it were executed.
///
/// Output:
/// - Successful validation under the Erlang profile.
///
/// Transformation:
/// - Confirms `@example target rust` is classified separately from generic
///   ignored examples and is skipped by target-profile matching.
#[test]
fn skips_non_matching_target_repl_doc_example() {
    let source = r#"module doc_examples.

/**
 * Demonstrates a native-only example.
 *
 * @example target rust
 * > native_only_call().
 */
pub value(): Int ->
    1.
"#;
    let module = terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

    validate_repl_doc_examples(
        &module,
        source,
        crate::DiagnosticFormat::default(),
        NativePolicy::SafeNativeOptional,
        TargetProfile::Erlang,
    )
    .expect("skip non-matching target example");
}

/// Verifies expected-error examples reject mismatched diagnostic text.
///
/// Inputs:
/// - One parsed module with an `@example error` block expecting the wrong
///   diagnostic code.
///
/// Output:
/// - `DoctestError` describing the expected-error mismatch.
///
/// Transformation:
/// - Executes the prompt through REPL-backed validation and requires the
///   expected output text to appear in the returned diagnostic.
#[test]
fn rejects_expected_error_repl_doc_example_mismatch() {
    let source = r#"module doc_examples.

/**
 * Demonstrates an unresolved name.
 *
 * @example error
 * > missing_value.
 * type_error
 */
pub value(): Int ->
    1.
"#;
    let module = terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

    let error = validate_repl_doc_examples(
        &module,
        source,
        crate::DiagnosticFormat::default(),
        NativePolicy::SafeNativeOptional,
        TargetProfile::Erlang,
    )
    .expect_err("reject expected-error mismatch");

    assert!(error.message.contains("error example mismatch"));
}
