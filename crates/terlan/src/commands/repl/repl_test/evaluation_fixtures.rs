use super::*;

use std::fs;

use super::super::bindings::{
    parse_repl_value_binding, repl_expression_with_bindings, ReplValueBinding,
};
use super::super::evaluation::{
    evaluate_repl_prompt_inputs, evaluate_repl_prompt_inputs_with_publication,
    repl_expression_module_source, run_repl_expression_in_session_with_output,
    validate_repl_seed_target_evidence, ReplCompilerService, ReplExpressionRequest,
};
use super::super::event::render_repl_json_event;
use super::super::help::is_repl_help_args;
use super::super::interactive::{
    interactive_repl_line_break, is_repl_debug_command, parse_repl_command_args, repl_debug_fields,
};
use super::super::source::{
    expression_parse_error_blocks_declaration_fallback, parse_repl_declaration,
};
use crate::runtime::vm::code_server::{VmCodeServer, VmCodeServerEvent, VmModuleGenerationState};
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::{ColorChoice, DiagnosticFormat};

pub(super) fn run_repl_expression_with_output(
    expression: &str,
    declarations: &[String],
    value_bindings: &[ReplValueBinding],
    module_name: &str,
    run_name: &str,
    temp_dir: &std::path::Path,
    diagnostic_format: DiagnosticFormat,
    native_policy: NativePolicy,
    target_profile: TargetProfile,
    output: &mut dyn FnMut(&str),
) -> Result<String, String> {
    let mut compiler_service = ReplCompilerService::default();
    run_repl_expression_in_session_with_output(
        &mut compiler_service,
        None,
        ReplExpressionRequest {
            expression,
            declarations,
            value_bindings,
            module_name,
            run_name,
            temp_dir,
            diagnostic_format,
            native_policy,
            target_profile,
        },
        output,
    )
}

pub(super) fn publish_repl_expression_generation(
    code_server: &mut VmCodeServer,
    expression: &str,
    declarations: &[String],
    value_bindings: &[ReplValueBinding],
    module_name: &str,
    run_name: &str,
) -> Result<VmCodeServerEvent, String> {
    let source = repl_expression_module_source(
        expression,
        declarations,
        value_bindings,
        module_name,
        run_name,
    );
    let event = code_server.publish_source(&format!("{module_name}.terl"), &source)?;
    code_server.purge_retired_generations(module_name)?;
    Ok(event)
}

/// Verifies REPL command-local help aliases are recognized.
///
/// Inputs:
/// - Synthetic command-local argument vectors for `--help` and `-h`.
///
/// Output:
/// - Test assertions only; no files are read or written.
///
/// Transformation:
/// - Exercises the REPL help detector without starting the interactive
///   command loop.
#[test]
pub(super) fn repl_help_args_accept_long_and_short_help() {
    assert!(is_repl_help_args(&["--help".to_string()]));
    assert!(is_repl_help_args(&["-h".to_string()]));
}

#[test]
pub(super) fn repl_rejects_local_constants_but_accepts_constant_imports() {
    let error = parse_repl_declaration("repl.Session", "const LOCAL: Int = 1.")
        .expect_err("REPL constants stay module-owned");
    assert!(error.0.contains("REPL_CONSTANT_DECLARATION"), "{error:?}");

    let declarations = parse_repl_declaration(
        "repl.Session",
        "import lifecycle.values.{ANSWER as IMPORTED}.",
    )
    .expect("REPL accepts public constant imports");
    assert_eq!(declarations.len(), 1);
}

/// Verifies interactive REPL line completion resets the terminal column.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Test passes when the raw-mode line ending is CRLF.
///
/// Transformation:
/// - Guards against result output being rendered under the typed prompt text
///   after `Enter` in raw terminal mode.
#[test]
pub(super) fn interactive_repl_line_break_returns_to_column_zero() {
    assert_eq!(interactive_repl_line_break(), "\r\n");
}

/// Verifies REPL command parsing has no runtime-selection surface.
///
/// Inputs:
/// - Command-local REPL arguments containing no runtime selector.
///
/// Output:
/// - Parsed seed path assertions and rejection of the retired selector.
///
/// Transformation:
/// - Keeps the native-image runtime mandatory at the command boundary.
#[test]
pub(super) fn repl_command_args_reject_runtime_selection() {
    let default = parse_repl_command_args(&[]).expect("parse default REPL arguments");
    assert_eq!(default.seed_path, None);
    assert!(!default.debug);

    assert_eq!(
        parse_repl_command_args(&["--runtime".into(), "vm".into()])
            .expect_err("runtime selector must be removed"),
        "unknown repl option: --runtime"
    );
}

/// Verifies REPL debug mode is a command-surface option.
///
/// Inputs:
/// - Command-local REPL arguments containing `--debug`.
///
/// Output:
/// - Parsed debug flag assertion.
///
/// Transformation:
/// - Keeps `terlc repl --debug` recognized before the interactive loop starts
///   so debugger entry points do not regress to unknown-option diagnostics.
#[test]
pub(super) fn repl_command_args_accept_debug_mode() {
    let parsed = parse_repl_command_args(&["--debug".into(), "src/Main.terl".into()])
        .expect("parse repl debug flag");

    assert!(parsed.debug);
    assert_eq!(parsed.seed_path.as_deref(), Some("src/Main.terl"));
}

/// Verifies the interactive REPL debug command is recognized explicitly.
///
/// Inputs:
/// - Trimmed REPL command strings.
///
/// Output:
/// - Boolean command classification assertions.
///
/// Transformation:
/// - Keeps `:debug` separate from unknown commands while avoiding accidental
///   wildcard matching of future debugger command arguments.
#[test]
pub(super) fn repl_debug_command_accepts_exact_spelling() {
    assert!(is_repl_debug_command(":debug"));
    assert!(!is_repl_debug_command(":debug now"));
    assert!(!is_repl_debug_command(":debugger"));
}

/// Verifies the enabled REPL debugger JSON event is stable.
///
/// Inputs:
/// - Reserved debugger event fields rendered with the REPL event helper.
///
/// Output:
/// - JSON assertions for code, command, and implementation state.
///
/// Transformation:
/// - Proves `terlc repl --debug` and `:debug` can share the same structured
///   debugger-unavailable event contract used by editor integrations.
#[test]
pub(super) fn repl_debug_json_event_is_stable() {
    let value: serde_json::Value = serde_json::from_str(&render_repl_json_event(
        "status",
        &repl_debug_fields(true),
        "VM debugger enabled",
    ))
    .expect("valid repl debug JSON event");

    assert_eq!(value["kind"], "status");
    assert_eq!(value["command"], "debug");
    assert_eq!(value["enabled"], true);
    assert_eq!(value["implemented"], true);
    assert_eq!(value["text"], "VM debugger enabled");
}

/// Verifies historical runtime selectors share one removed-option diagnostic.
///
/// Inputs:
/// - Command-local REPL arguments selecting `--runtime beam`.
/// - Experimental flag enabled.
///
/// Output:
/// - Usage error text.
///
/// Transformation:
/// - Prevents any alternate runtime path from remaining public REPL surface.
#[test]
pub(super) fn repl_command_args_reject_beam_runtime() {
    let error = parse_repl_command_args(&["--runtime".into(), "beam".into()])
        .expect_err("beam runtime should be removed");

    assert_eq!(error, "unknown repl option: --runtime");
}

/// Verifies REPL seed loading rejects source that requires JavaScript.
///
/// Inputs:
/// - Temporary seed module importing `std.js.Promise`.
///
/// Output:
/// - Stable target-inference diagnostic.
///
/// Transformation:
/// - Exercises the startup and `:load` seed-target preflight so the VM REPL
///   cannot accept non-VM declarations and fail later during expression
///   evaluation.
#[test]
pub(super) fn repl_seed_target_inference_rejects_js_source_evidence() {
    let root = make_repl_test_dir("seed_target_js_evidence");
    let source = root.join("Main.terl");
    fs::write(
        &source,
        "\
module app.Main.

import type std.js.Promise.

pub accepts(value: Promise[Int]): Promise[Int] ->
    value.
",
    )
    .expect("write js seed source");

    let message =
        validate_repl_seed_target_evidence(Some(&source.to_string_lossy()), TargetProfile::Vm)
            .expect_err("expected JS seed target rejection");

    assert!(
        message.contains("seed source evidence requires `js.shared`"),
        "{message}"
    );
    fs::remove_dir_all(root).expect("remove temp dir");
}

/// Verifies explicit non-VM profiles cannot select REPL execution.
///
/// Inputs:
/// - Target-neutral seed module and explicit JS target profile.
///
/// Output:
/// - Stable VM-only runtime diagnostic.
///
/// Transformation:
/// - Confirms REPL seed validation keeps target-neutral code on the VM
///   runtime instead of allowing a non-VM REPL execution mode.
#[test]
pub(super) fn repl_seed_target_inference_rejects_explicit_js_profile_for_vm_source() {
    let root = make_repl_test_dir("seed_target_explicit_js");
    let source = root.join("Main.terl");
    fs::write(
        &source,
        "\
module app.Main.

pub value(): Int ->
    1.
",
    )
    .expect("write vm seed source");

    let message = validate_repl_seed_target_evidence(
        Some(&source.to_string_lossy()),
        TargetProfile::JsShared,
    )
    .expect_err("expected explicit JS target conflict");

    assert!(
        message.contains(
            "REPL runtime `vm` executes VM programs, but explicit target `js.shared` was requested"
        ),
        "{message}"
    );
    fs::remove_dir_all(root).expect("remove temp dir");
}

/// Verifies simple REPL expressions execute through the experimental VM path.
///
/// Inputs:
/// - One arithmetic expression compiled as a synthetic REPL function.
///
/// Output:
/// - Rendered integer result from the in-process Terlan VM.
///
/// Transformation:
/// - Compiles the expression with the VM profile and executes its admitted
///   native image.
#[test]
pub(super) fn repl_expression_runs_arithmetic_through_vm_runtime() {
    let root = make_repl_test_dir("vm_runtime_arithmetic");
    let mut output = Vec::new();
    let value = run_repl_expression_with_output(
        "1 + 2",
        &[],
        &[],
        "repl_vm_test",
        "repl_eval_1",
        &root,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
        &mut |line| output.push(line.to_string()),
    )
    .expect("run vm repl expression");

    assert_eq!(output, Vec::<String>::new());
    assert_eq!(value, "3");
    fs::remove_dir_all(root).expect("remove temp dir");
}

/// Proves seed/imported compiler constants are substituted in REPL snippets.
#[test]
pub(super) fn value_lifecycle_public_constants_execute_in_repl_without_runtime_symbols() {
    let root = make_repl_test_dir("value_lifecycle_public_constants");
    let declarations = vec![
        "pub const ANSWER: Int = 40 + 2.".to_string(),
        "pub type Status: Int = OK = 200 | NOT_FOUND = 404.".to_string(),
        "pub trait HasCode[T] { const CODE: Int = ANSWER. }.".to_string(),
        "pub impl HasCode[Int] for Int { }.".to_string(),
    ];
    let mut output = Vec::new();
    let value = run_repl_expression_with_output(
        "ANSWER",
        &declarations,
        &[],
        "repl_value_lifecycle",
        "repl_eval_constants",
        &root,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
        &mut |line| output.push(line.to_string()),
    )
    .expect("execute a compiler-substituted public constant in the REPL");

    assert_eq!(value, "42");
    assert!(output.is_empty());
    fs::remove_dir_all(root).expect("remove temp dir");
}

/// Verifies the experimental VM REPL path does not emit runtime artifacts.
///
/// Inputs:
/// - One literal expression compiled as a synthetic REPL function.
///
/// Output:
/// - Rendered integer result and workspace file assertions.
///
/// Transformation:
/// - Exercises the VM runtime branch and confirms it does not invoke the
///   VM path that writes `.erl` or `.beam` artifacts.
#[test]
pub(super) fn repl_expression_vm_runtime_does_not_emit_erlang_artifacts() {
    let root = make_repl_test_dir("vm_runtime_no_erlang");
    let mut output = Vec::new();
    let value = run_repl_expression_with_output(
        "42",
        &[],
        &[],
        "repl_vm_artifact_test",
        "repl_eval_1",
        &root,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
        &mut |line| output.push(line.to_string()),
    )
    .expect("run vm repl expression");

    let entries = fs::read_dir(&root)
        .expect("read repl temp dir")
        .map(|entry| entry.expect("read entry").path())
        .collect::<Vec<_>>();
    assert_eq!(output, Vec::<String>::new());
    assert_eq!(value, "42");
    assert!(
        entries.iter().any(|path| path
            .extension()
            .is_some_and(|extension| extension == "terl")),
        "VM REPL should still write the synthetic Terlan source for compilation"
    );
    assert!(
        !entries
            .iter()
            .any(|path| path.extension().is_some_and(|extension| extension == "erl")),
        "VM REPL must not emit Vm source"
    );
    assert!(
        !entries.iter().any(|path| path
            .extension()
            .is_some_and(|extension| extension == "beam")),
        "VM REPL must not emit VM bytecode"
    );
    fs::remove_dir_all(root).expect("remove temp dir");
}

/// Verifies REPL-generated source can publish VM hot-reload generations.
///
/// Inputs:
/// - Two expression entries for the same synthetic REPL module.
/// - Caller-owned VM code server.
///
/// Output:
/// - Initial publish, hot-reload, and retired-generation purge events.
///
/// Transformation:
/// - Exercises the reusable REPL-to-code-server publication boundary that a
///   future interactive REPL or dev watcher can call without restarting the VM.
#[test]
pub(super) fn repl_publish_expression_generation_records_hot_reload_event() {
    let mut code_server = VmCodeServer::default();

    let published = publish_repl_expression_generation(
        &mut code_server,
        "1",
        &[],
        &[],
        "repl_hot_reload_test",
        "repl_eval_1",
    )
    .expect("first REPL generation should publish");
    let VmCodeServerEvent::Published {
        ref module,
        generation: generation_1,
    } = published
    else {
        panic!("expected initial REPL publish event");
    };
    assert_eq!(module, "repl_hot_reload_test");

    let reloaded = publish_repl_expression_generation(
        &mut code_server,
        "2",
        &[],
        &[],
        "repl_hot_reload_test",
        "repl_eval_2",
    )
    .expect("second REPL generation should hot reload");

    let VmCodeServerEvent::HotReloaded {
        previous_generation,
        previous_state,
        active_generation,
        ..
    } = reloaded
    else {
        panic!("expected REPL hot-reload event");
    };
    assert_eq!(previous_generation, generation_1);
    assert_eq!(previous_state, VmModuleGenerationState::Retired);
    assert_ne!(active_generation, generation_1);

    let events = code_server.event_snapshots();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].event, published);
    assert_eq!(events[1].event, reloaded);
    assert_eq!(events[2].sequence, 3);
    assert_eq!(
        events[2].event,
        VmCodeServerEvent::GenerationPurged {
            module: "repl_hot_reload_test".to_string(),
            generation: generation_1,
        }
    );
}

#[test]
pub(super) fn value_lifecycle_constant_change_publishes_one_new_repl_generation() {
    let mut code_server = VmCodeServer::default();
    let first = publish_repl_expression_generation(
        &mut code_server,
        "ANSWER",
        &["pub const ANSWER: Int = 1.".to_string()],
        &[],
        "repl_constant_reload",
        "repl_eval",
    )
    .expect("publish first constant generation");
    let VmCodeServerEvent::Published {
        generation: first_generation,
        ..
    } = first
    else {
        panic!("expected initial publication");
    };

    let second = publish_repl_expression_generation(
        &mut code_server,
        "ANSWER",
        &["pub const ANSWER: Int = 2.".to_string()],
        &[],
        "repl_constant_reload",
        "repl_eval",
    )
    .expect("publish changed constant generation");
    let VmCodeServerEvent::HotReloaded {
        previous_generation,
        active_generation,
        ..
    } = second
    else {
        panic!("constant change must hot reload dependents");
    };
    assert_eq!(previous_generation, first_generation);
    assert_ne!(active_generation, first_generation);
}

/// Verifies the non-interactive prompt path publishes VM generations.
///
/// Inputs:
/// - Two expression prompt entries evaluated through the doc/automation REPL
///   prompt helper.
///
/// Output:
/// - Rendered prompt values and ordered VM code-server events.
///
/// Transformation:
/// - Proves the prompt evaluator itself, not only the low-level publication
///   helper, publishes source-level REPL generations that can hot reload in a
///   long-lived VM code server.
#[test]
pub(super) fn repl_prompt_inputs_publish_vm_generations_for_expressions() {
    let (outputs, events) = evaluate_repl_prompt_inputs_with_publication(
        &["1.".to_string(), "2.".to_string()],
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("evaluate and publish repl prompts");

    assert_eq!(outputs, vec![vec!["1".to_string()], vec!["2".to_string()]]);
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[1].sequence, 2);
    assert_eq!(events[2].sequence, 3);
    assert!(matches!(
        events[0].event,
        VmCodeServerEvent::Published { .. }
    ));
    assert!(matches!(
        events[1].event,
        VmCodeServerEvent::HotReloaded { .. }
    ));
    assert!(matches!(
        events[2].event,
        VmCodeServerEvent::GenerationPurged { .. }
    ));
}

/// Verifies REPL help detection does not consume seed paths.
///
/// Inputs:
/// - Synthetic command-local argument vectors for empty args, one seed
///   path, and malformed extra arguments.
///
/// Output:
/// - Test assertions only; no files are read or written.
///
/// Transformation:
/// - Keeps REPL help routing exact so normal seed loading and malformed
///   argument diagnostics remain owned by the main command path.
#[test]
pub(super) fn repl_help_args_reject_non_help_invocations() {
    assert!(!is_repl_help_args(&[]));
    assert!(!is_repl_help_args(&["src/main.terl".to_string()]));
    assert!(!is_repl_help_args(&[
        "--help".to_string(),
        "src/main.terl".to_string()
    ]));
}

/// Verifies that REPL binding syntax captures a simple persistent name.
///
/// Inputs:
/// - A terminator-stripped REPL entry with shape `let name = expr`.
///
/// Output:
/// - Parsed name and value expression.
///
/// Transformation:
/// - Exercises the REPL-only binding parser without invoking the full
///   interactive command loop.
#[test]
pub(super) fn repl_value_binding_parser_accepts_simple_binding() {
    let binding = parse_repl_value_binding("let total = 1 + 2").unwrap();

    assert_eq!(binding.pattern, "total");
    assert_eq!(binding.value, "1 + 2");
}

/// Verifies that REPL binding syntax accepts destructuring patterns.
///
/// Inputs:
/// - A terminator-stripped REPL entry with a tuple pattern on the left side.
///
/// Output:
/// - Parsed pattern text and value expression.
///
/// Transformation:
/// - Keeps the REPL persistence parser broad enough for formal pattern
///   validation to happen through the compiler path instead of local name
///   filtering.
#[test]
pub(super) fn repl_value_binding_parser_accepts_tuple_pattern() {
    let binding = parse_repl_value_binding("let {head, _} = pair").unwrap();

    assert_eq!(binding.pattern, "{head, _}");
    assert_eq!(binding.value, "pair");
}

/// Verifies that full Terlan `let` expressions are left to source parsing.
///
/// Inputs:
/// - A terminator-stripped source `let` expression containing `;`.
///
/// Output:
/// - `None`, indicating the REPL did not treat the entry as persistent
///   session state.
///
/// Transformation:
/// - Keeps the REPL-only `let name = expr.` entry form separate from normal
///   Terlan let expressions such as `let x = 1; x + 1`.
#[test]
pub(super) fn repl_value_binding_parser_rejects_source_let_expression() {
    assert!(parse_repl_value_binding("let x = 1; x + 1").is_none());
}

/// Verifies assignment-shaped expression diagnostics do not fall through to
/// declaration parsing.
///
/// Inputs:
/// - The parser diagnostic emitted for `a = a + 1`.
///
/// Output:
/// - Test passes when the REPL treats the expression error as definitive.
///
/// Transformation:
/// - Keeps interactive `a = ...` failures focused on binding/equality/matching
///   guidance instead of adding an unrelated declaration parse error.
#[test]
pub(super) fn repl_assignment_expression_error_blocks_declaration_fallback() {
    assert!(expression_parse_error_blocks_declaration_fallback(
        "plain `=` is not assignment or pattern matching in Terlan; use `let name = value` to bind"
    ));
    assert!(!expression_parse_error_blocks_declaration_fallback(
        "unexpected token Pub in expression"
    ));
}

/// Verifies malformed string captures do not produce declaration noise.
///
/// Inputs:
/// - Every parser-owned malformed string-capture diagnostic.
///
/// Output:
/// - Each diagnostic is classified as a definitive expression error.
///
/// Transformation:
/// - Keeps capture-heavy REPL failures aligned with file diagnostics instead
///   of appending an unrelated module-declaration parse error.
#[test]
pub(super) fn repl_string_capture_errors_block_declaration_fallback() {
    for message in [
        "adjacent string captures require a literal separator",
        "unterminated string capture pattern",
        "empty string capture pattern",
        "string capture type annotation cannot be empty",
    ] {
        assert!(expression_parse_error_blocks_declaration_fallback(message));
    }
}

/// Verifies malformed capture diagnostics remain stable in REPL evaluation.
///
/// Inputs:
/// - A case expression containing adjacent capture slots.
///
/// Output:
/// - One expression-owned diagnostic with no declaration fallback text.
///
/// Transformation:
/// - Locks REPL diagnostics to the parser contract used by files and editor
///   tooling for the same malformed pattern.
#[test]
pub(super) fn repl_prompt_inputs_reject_adjacent_string_captures_without_declaration_noise() {
    let error = evaluate_repl_prompt_inputs(
        &[r#"case "ab" { "${left}${right}" -> left; _ -> "" }."#.to_string()],
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect_err("adjacent string captures should fail in repl");

    assert_eq!(
        error,
        "REPL doc expression parse error: adjacent string captures require a literal separator"
    );
    assert!(!error.contains("declaration parse error"));
}

/// Verifies that persisted bindings lower through ordinary Terlan `let`.
///
/// Inputs:
/// - Current expression source and two persisted REPL value bindings.
///
/// Output:
/// - Generated source expression with persisted bindings evaluated first.
///
/// Transformation:
/// - Converts REPL state into the compiler-owned source form used before
///   parsing, typechecking, CoreIR lowering, and evaluator execution.
#[test]
pub(super) fn repl_expression_with_bindings_builds_source_let_expression() {
    let expression = repl_expression_with_bindings(
        "x + y",
        &[
            ReplValueBinding {
                pattern: "x".to_string(),
                value: "1".to_string(),
            },
            ReplValueBinding {
                pattern: "y".to_string(),
                value: "2".to_string(),
            },
        ],
    );

    assert_eq!(expression, "let x = (1); let y = (2); x + y");
}

/// Verifies persisted lambda bindings remain separated from the later body.
///
/// Inputs:
/// - Current function-value call expression and one persisted lambda binding.
///
/// Output:
/// - Generated source expression with the lambda parenthesized.
///
/// Transformation:
/// - Protects anonymous-function bodies from consuming the hidden REPL
///   semicolon separator when prompt state is converted into a Terlan `let`.
#[test]
pub(super) fn repl_expression_with_bindings_parenthesizes_lambda_binding_values() {
    let expression = repl_expression_with_bindings(
        "a(10)",
        &[ReplValueBinding {
            pattern: "a".to_string(),
            value: "(x) -> x + x".to_string(),
        }],
    );

    assert_eq!(expression, "let a = ((x) -> x + x); a(10)");
}

/// Verifies REPL prompt evaluation supports persisted function values.
///
/// Inputs:
/// - One REPL binding prompt defining a lambda value.
/// - One later prompt invoking the persisted function value.
///
/// Output:
/// - First prompt evaluates to `Unit`; second prompt evaluates to `20`.
///
/// Transformation:
/// - Exercises the full prompt path used by docs and non-interactive REPL
///   checks: parse prompt terminators, persist `let` bindings, compile through
///   CoreIR, and evaluate function-value invocation without VM execution.
#[test]
pub(super) fn repl_prompt_inputs_apply_persisted_lambda_binding() {
    let outputs = evaluate_repl_prompt_inputs(
        &["let a = (x) -> x + x.".to_string(), "a(10).".to_string()],
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("evaluate repl prompts");

    assert_eq!(
        outputs,
        vec![vec!["Unit".to_string()], vec!["20".to_string()]]
    );
}
