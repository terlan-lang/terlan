//! Source-to-object coverage for suspension-capable calls inside case branches.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::{emit_native_application_object, NativeModule};

/// A synchronous guard prefix after suspension must retain its terminal union ABI.
#[test]
fn grouped_guards_resume_with_a_managed_nullary_option() {
    let source = r#"
module grouped_option_resume_source.
pub type None = Atom["none"].
pub type Some[T] = {Atom["some"], value: T}.
pub type Option[T] = None | Some[T].
@compiler.native {probe.read}
read(): Bool -> native.
pub evaluate(): Option[Int] ->
    let { true <- read(); true <- read() } else { _ -> Some(1) };
    None.
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse grouped Option guards");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower grouped Option guards");
    let mut resumed = 0;
    for continuation in modules.iter().flat_map(|module| &module.continuations) {
        if continuation.source_function == "evaluate" {
            resumed += 1;
            assert!(matches!(
                continuation.return_type,
                super::NativeType::ManagedRef(_)
            ));
            assert_managed_terminal(&continuation.body);
        }
    }
    assert!(resumed > 0, "fixture must exercise a resumed entry");
    emit_native_application_object("grouped_option_resume", &modules)
        .expect("emit managed Option completion");
}

/// Checks terminal positions only; scalar atoms remain valid in predicates.
fn assert_managed_terminal(value: &super::NativeExpr) {
    match value {
        super::NativeExpr::Let { body, .. } => assert_managed_terminal(body),
        super::NativeExpr::If { clauses } => {
            for (_, body) in clauses {
                assert_managed_terminal(body);
            }
        }
        super::NativeExpr::AtomLiteral(value) => panic!("unboxed managed result: {value}"),
        _ => {}
    }
}

/// A discarded Unit-valued assertion check must still terminate on failure.
#[test]
fn script_assertion_failure_survives_a_discarded_unit_check() {
    let syntax = crate::terlan_syntax::parse_script_as_syntax_output(
        "import std.vm.Process.\nassert(false);\nProcess.yield_now();\nUnit.\n",
        "fixture.AssertFailure",
    )
    .expect("parse failed assertion script");
    let interfaces = crate::terlan_hir::checked_in_std_interfaces_for_module(&syntax);
    let resolved =
        crate::terlan_hir::resolve_syntax_module_output_with_interfaces(&syntax, &interfaces)
            .module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let body = core
        .functions
        .iter()
        .find(|function| function.name == "main")
        .and_then(|function| function.clauses.first())
        .and_then(|clause| clause.body.core_expr.as_ref())
        .expect("main core body");
    assert!(
        body.contract_text().contains("vm.process.fail"),
        "{}",
        body.contract_text()
    );
    let modules = NativeModule::lower_application(&[&core]).expect("lower failed assertion");
    let mut failures = 0;
    for module in &modules {
        for body in module
            .functions
            .iter()
            .map(|function| &function.body)
            .chain(
                module
                    .continuations
                    .iter()
                    .map(|continuation| &continuation.body),
            )
        {
            super::call_composition::walk_native_expr(body, &mut |expression| {
                if matches!(
                    expression,
                    super::NativeExpr::Suspend {
                        operation: super::NativeTransitionOperation::Failure,
                        ..
                    }
                ) {
                    failures += 1;
                }
            });
        }
    }
    assert!(
        failures > 0,
        "failure was discarded from {}",
        body.contract_text()
    );
}

/// An outer Boolean branch must not become an input to an inner Result join.
#[test]
fn conditional_fallible_binding_keeps_its_join_inside_the_outer_branch() {
    let binding = "(let Ok(value) <- if { choice -> read_left(); true -> read_right() } else { _ -> false }; value > 0)";
    for expression in [
        format!("if {{ enabled -> {binding}; true -> false }}"),
        format!("if {{ not enabled -> false; true -> {binding} }}"),
        format!("enabled and {binding}"),
        format!("enabled or {binding}"),
    ] {
        let source = format!(
            r#"
module nested_completion_source.
pub type Ok[T] = {{Atom["ok"], value: T}}.
pub type Err[E] = {{Atom["error"], reason: E}}.
pub type Result[T, E] = Ok[T] | Err[E].
@compiler.native {{probe.left}}
read_left(): Result[Int, String] -> native.
@compiler.native {{probe.right}}
read_right(): Result[Int, String] -> native.
pub evaluate(enabled: Bool, choice: Bool): Bool -> {expression}.
"#,
        );
        let syntax = parse_module_as_syntax_output(&source).expect("parse nested completion");
        let resolved = resolve_syntax_module_output(&syntax).module;
        let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let core = lower_syntax_module_output_to_core(&syntax, &resolved);
        let modules = NativeModule::lower_application(&[&core])
            .unwrap_or_else(|error| panic!("{expression}: {error}"));
        emit_native_application_object("nested_completion", &modules)
            .expect("emit branch-local completion object");
    }
}

/// A transparent constructor tag stays available after evaluating its payload.
#[test]
fn mixed_result_branches_retain_tags_across_suspending_payload_calls() {
    for expression in [
        "if { enabled -> read_result(); true -> Ok(read()) }",
        "if { enabled -> Ok(read()); true -> read_result() }",
    ] {
        let source = format!(
            r#"
module suspending_result_tag_source.

pub type Ok[T] = {{Atom["ok"], value: T}}.
pub type Err[E] = {{Atom["error"], reason: E}}.
pub type Result[T, E] = Ok[T] | Err[E].

@compiler.native {{probe.read}}
read(): Int -> native.

@compiler.native {{probe.read_result}}
read_result(): Result[Int, String] -> native.

pub evaluate(enabled: Bool): Result[Int, String] ->
    let result = {expression};
    result.
"#,
        );
        let syntax = parse_module_as_syntax_output(&source).expect("parse yielding Result");
        let resolved = resolve_syntax_module_output(&syntax).module;
        let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let core = lower_syntax_module_output_to_core(&syntax, &resolved);
        let modules = NativeModule::lower_application(&[&core])
            .unwrap_or_else(|error| panic!("{expression}: {error}"));
        emit_native_application_object("suspending_result_tag", &modules)
            .expect("emit yielding Result constructor object");
    }
}

/// Case-valued conditions must compose their scrutinee before choosing a branch.
#[test]
fn inline_case_condition_composes_a_suspending_result_scrutinee() {
    let syntax = parse_module_as_syntax_output(
        r#"
module inline_case_condition_source.

pub type Ok[T] = {Atom["ok"], value: T}.
pub type Err[E] = {Atom["error"], reason: E}.
pub type Result[T, E] = Ok[T] | Err[E].

@compiler.native {probe.read}
read(): Result[Int, String] -> native.

pub evaluate(): Int ->
    if {
        case read() { Ok(value) -> value == 0; Err(_reason) -> false } -> 1;
        true -> 0
    }.
"#,
    )
    .expect("parse inline case condition");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core])
        .expect("compose the case scrutinee inside a conditional");
    assert!(modules
        .iter()
        .any(|module| !module.continuations.is_empty()));
    emit_native_application_object("inline_case_condition", &modules)
        .expect("emit inline case condition object");
}

#[test]
fn non_tail_recursive_native_calls_retain_their_post_call_continuation() {
    let syntax = parse_module_as_syntax_output(
        r#"
module suspending_recursive_source.

@compiler.native {probe.read}
read(value: Int): Int -> native.

pub evaluate(count: Int): Int ->
    if {
        count == 0 -> 0;
        true ->
            let value = read(count);
            let rest = evaluate(count - 1);
            value + rest
    }.
"#,
    )
    .expect("parse non-tail recursive suspension fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core])
        .expect("lower recursive suspended call with pending result consumption");
    emit_native_application_object("suspending_recursive_source", &modules)
        .expect("emit recursive suspended call with pending result consumption");
}

#[test]
fn empty_first_nested_list_of_structs_survives_a_suspending_prefix() {
    let syntax = parse_module_as_syntax_output(
        r#"
module suspending_nested_list_source.

pub struct Node { value: Int }.

@compiler.native {probe.read}
read(): Bool -> native.

@compiler.native {probe.consume}
consume(values: List[List[Node]]): Bool -> native.

pub evaluate(): Bool ->
    let matrix = [[], [Node(value = 7)], []];
    let ready = read();
    if { ready -> consume(matrix); true -> false }.
"#,
    )
    .expect("parse nested empty-list suspension fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core])
        .expect("lower nested empty-list prefix with its checked element layout");
    emit_native_application_object("suspending_nested_list_source", &modules)
        .expect("emit nested empty-list suspension object");
}

#[test]
fn suspending_struct_result_projection_composes_before_boolean_control() {
    for expression in [
        "not read_summary().valid and read_summary().valid",
        "not read_summary()#Summary.valid or read_summary()#Summary.valid",
        "if { read_summary().valid -> read_summary().valid; true -> false }",
    ] {
        let source = format!(
            r#"
module suspending_projection_source.

pub struct Summary {{ valid: Bool }}.

@compiler.native {{probe.read}}
read(): Bool -> native.

read_summary(): Summary ->
    let valid = read();
    Summary(valid = valid).

pub evaluate(): Bool -> {expression}.
"#
        );
        let syntax = parse_module_as_syntax_output(&source)
            .expect("parse suspending struct projection fixture");
        let resolved = resolve_syntax_module_output(&syntax).module;
        let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
        assert!(diagnostics.is_empty(), "{expression}: {diagnostics:#?}");
        let core = lower_syntax_module_output_to_core(&syntax, &resolved);
        let modules = NativeModule::lower_application(&[&core])
            .unwrap_or_else(|error| panic!("{expression}: {error}"));
        emit_native_application_object("suspending_projection_source", &modules)
            .expect("emit suspending struct projection object");
    }
}

#[test]
fn grouped_fallible_bindings_share_fallbacks_across_scalar_and_result_types() {
    let syntax = parse_module_as_syntax_output(
        r#"
module suspending_grouped_fallback_source.

pub type Ok[T] = {Atom["ok"], value: T}.
pub type Err[E] = {Atom["error"], reason: E}.
pub type Result[T, E] = Ok[T] | Err[E].

@compiler.native {probe.read}
read(): Result[Int, String] -> native.

pub evaluate(enabled: Bool): Result[Int, String] ->
    let {
        true <- enabled;
        Ok(value) <- read()
    } else {
        Err(reason) -> Err(reason);
        _ -> Err("disabled")
    };
    Ok(value).
"#,
    )
    .expect("parse heterogeneous grouped fallback fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core])
        .expect("lower typed heterogeneous fallbacks without matching a tuple on Bool");
    emit_native_application_object("suspending_grouped_fallback_source", &modules)
        .expect("emit heterogeneous grouped fallback object");
}

#[test]
fn exhaustive_valued_union_arms_compose_suspending_native_calls() {
    let syntax = parse_module_as_syntax_output(
        r#"
module valued_union_suspending_case_source.

pub type Mode: Int = LOWER = 0 | UPPER = 1.

@compiler.native {probe.lower}
lower(): Int -> native.

@compiler.native {probe.upper}
upper(): Int -> native.

pub select(mode: Mode): Int ->
    case mode {
        Mode.LOWER -> lower();
        Mode.UPPER -> upper()
    }.
"#,
    )
    .expect("parse valued-union suspension fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core])
        .expect("lower exhaustive valued-union suspension application");

    emit_native_application_object("valued_union_suspending_case_source", &modules)
        .expect("emit valued-union suspension object");
}

#[test]
fn suspension_aware_case_continuations_keep_their_hidden_abi_arguments() {
    let syntax = parse_module_as_syntax_output(
        r#"
module suspending_case_source.

@compiler.native {probe.read}
read(): Int -> native.

read_wrapped(): Int ->
    read().

@compiler.native {probe.inspect}
inspect(value: Int): Bool -> native.

pub evaluate(): Bool ->
    case read_wrapped() {
        0 -> false;
        value -> inspect(value)
    }.
"#,
    )
    .expect("parse suspending case fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules =
        NativeModule::lower_application(&[&core]).expect("lower suspension-aware case application");

    emit_native_application_object("suspending_case_source", &modules)
        .expect("emit suspension-aware case object");
}

/// Verifies nested native calls evaluate inner suspensions before tail calls.
///
/// Inputs:
/// - Two package-native functions where the inner call supplies the outer
///   call's sole argument.
///
/// Output:
/// - Source lowers through the application and native-object pipelines without
///   retaining an ordinary suspension-capable call in a tail-call argument.
///
/// Transformation:
/// - Forces continuation composition to run before terminal tail-call lowering
///   when an argument can suspend.
#[test]
fn nested_suspending_native_call_arguments_lower_as_continuations() {
    let syntax = parse_module_as_syntax_output(
        r#"
module nested_suspending_native_source.

@compiler.native {probe.inner}
inner(value: Int): Int -> native.

@compiler.native {probe.outer}
outer(value: Int): Bool -> native.

pub evaluate(): Bool ->
    outer(inner(7)).
"#,
    )
    .expect("parse nested suspension fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules =
        NativeModule::lower_application(&[&core]).expect("lower nested suspension application");

    emit_native_application_object("nested_suspending_native_source", &modules)
        .expect("emit nested suspension object");
}

/// Verifies sequential case bindings can suspend in both scrutinees and arms.
///
/// Inputs:
/// - Two native reads stored through separate case-valued lexical bindings.
/// - A native release call in each nonzero case arm.
///
/// Output:
/// - The complete source lowers through NativeIR and object emission.
///
/// Transformation:
/// - Proves admission and suspension-aware case lowering agree on nested
///   continuations inside sequential let bindings.
#[test]
fn sequential_suspending_case_bindings_lower_as_continuations() {
    let syntax = parse_module_as_syntax_output(
        r#"
module sequential_suspending_case_source.

@compiler.native {probe.read}
read(): Int -> native.

@compiler.native {probe.release}
release(value: Int): Unit -> native.

pub evaluate(): Bool ->
    let first = case read() {
        0 ->
            true;
        value ->
            let _released = release(value);
            false
    };
    let second = case read() {
        0 ->
            true;
        value ->
            let _released = release(value);
            false
    };
    first and second.
"#,
    )
    .expect("parse sequential suspending case fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core])
        .expect("lower sequential suspending case application");

    emit_native_application_object("sequential_suspending_case_source", &modules)
        .expect("emit sequential suspending case object");
}

/// Verifies a structured case carries its lexical suffix into each arm.
///
/// Inputs:
/// - A suspending wrapper selected from one `Result` constructor arm.
/// - A later native cleanup transition after the case-valued binding.
///
/// Output:
/// - The source lowers through NativeIR and native object emission.
///
/// Transformation:
/// - Proves structured control distributes the later continuation into the
///   selected arm, allowing the wrapper suspension and cleanup to compose.
#[test]
fn structured_case_value_composes_suspending_wrapper_with_later_cleanup() {
    let syntax = parse_module_as_syntax_output(
        r#"
module structured_case_continuation_source.

pub type Ok[T] = {Atom["ok"], value: T}.
pub type Err[E] = {Atom["error"], reason: E}.
pub type Result[T, E] = Ok[T] | Err[E].

@compiler.native {probe.open}
open(): Result[Int, String] -> native.

@compiler.native {probe.read}
read(handle: Int): Bool -> native.

read_wrapped(handle: Int): Bool ->
    read(handle).

@compiler.native {probe.cleanup}
cleanup(): Unit -> native.

pub evaluate(): Bool ->
    let opened = open();
    let result = case opened {
        Ok(handle) -> read_wrapped(handle);
        Err(_reason) -> false
    };
    let _cleaned = cleanup();
    result.
"#,
    )
    .expect("parse structured-case continuation fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core])
        .expect("lower structured-case continuation application");

    emit_native_application_object("structured_case_continuation_source", &modules)
        .expect("emit structured-case continuation object");
}

/// Verifies a gated suspension retains a managed union representation at its
/// shared continuation join.
///
/// The false arm uses the compact source spelling `None`, while the true arm
/// constructs `Some(value)` after a native transition. The checked
/// `Option[Int]` type must govern both arms before the joined value is matched.
#[test]
fn gated_suspending_option_materializes_nullary_managed_variant_at_join() {
    let syntax = parse_module_as_syntax_output(
        r#"
module gated_suspending_option_source.

pub type None = Atom["none"].
pub type Some[T] = {Atom["some"], value: T}.
pub type Option[T] = None | Some[T].

@compiler.native {probe.read}
read(): Int -> native.

some_after_read(): Option[Int] ->
    Some(read()).

pub evaluate(enabled: Bool): Bool ->
    let value = if {
        enabled -> some_after_read();
        true -> None
    };
    value != None.
"#,
    )
    .expect("parse gated suspending Option fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core])
        .expect("lower gated suspending Option application");

    emit_native_application_object("gated_suspending_option_source", &modules)
        .expect("emit gated suspending Option object");
}

/// Verifies a structured managed result after a suspension is not mistaken
/// for a literal collection value by continuation lowering.
#[test]
fn suspended_prefix_can_resume_into_an_option_of_struct_case() {
    let syntax = parse_module_as_syntax_output(
        r#"
module suspended_option_struct_source.

pub type None = Atom["none"].
pub type Some[T] = {Atom["some"], value: T}.
pub type Option[T] = None | Some[T].

pub struct Host {
    operating_system: String,
    architecture: String
}.

@compiler.native {probe.host}
host_name(): String -> native.

normalize(value: String): Option[String] -> if {
    value == "linux" -> Some(value);
    true -> None
}.

pub detect(): Option[Host] ->
    let raw = host_name();
    case normalize(raw) {
        Some(name) -> Some(Host(operating_system = name, architecture = "x86_64"));
        None -> None
    }.
"#,
    )
    .expect("parse suspended Option-of-struct fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core])
        .expect("lower suspended Option-of-struct application");

    emit_native_application_object("suspended_option_struct_source", &modules)
        .expect("emit suspended Option-of-struct object");
}

/// Verifies a suspending Result continuation can materialize records in both
/// the success value and the typed error value.
///
/// This is the deploy-plan loading shape used by Terlan Cloud: the host read
/// suspends, success delegates to a parser returning `Result[Plan, Problem]`,
/// and failure constructs the nominal error record before wrapping it in Err.
#[test]
fn suspended_result_can_resume_with_struct_value_and_struct_error() {
    let syntax = parse_module_as_syntax_output(
        r#"
module suspended_result_struct_source.

pub type Ok[T] = {Atom["ok"], value: T}.
pub type Err[E] = {Atom["error"], reason: E}.
pub type Result[T, E] = Ok[T] | Err[E].

pub type FileRead.

pub struct Problem {
    code: Atom,
    message: String,
    field: String
}.

pub struct Plan {
    schema: String
}.

@compiler.native {probe.read}
read(path: String): Result[String, String] -> native.

problem(code: Atom, message: String, field: String): Problem ->
    Problem {code: code, message: message, field: field}.

parse(_text: String): Result[Plan, Problem] ->
    Ok(Plan {schema: "terlan-cloud-deploy-plan-v1"}).

pub load(path: String): Result[Plan, Problem] ->
    case read(path) {
        Ok(text) -> parse(text);
        Err(reason) -> Err(problem(FileRead, reason, path))
    }.
"#,
    )
    .expect("parse suspended Result-of-struct fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core])
        .expect("lower suspended Result-of-struct application");

    emit_native_application_object("suspended_result_struct_source", &modules)
        .expect("emit suspended Result-of-struct object");
}

/// Verifies fixed record construction retains the checked type of `None`
/// fields while lowering a structured result after a suspension.
#[test]
fn suspended_result_record_can_materialize_typed_none_fields() {
    let syntax = parse_module_as_syntax_output(
        r#"
module suspended_result_option_record_source.

pub type None = Atom["none"].
pub type Some[T] = {Atom["some"], value: T}.
pub type Option[T] = None | Some[T].
pub type Ok[T] = {Atom["ok"], value: T}.
pub type Err[E] = {Atom["error"], reason: E}.
pub type Result[T, E] = Ok[T] | Err[E].

pub struct Source {
    kind: String,
    path: Option[String],
    url: Option[String]
}.

@compiler.native {probe.read}
read(path: String): Result[String, String] -> native.

pub load(path: String): Result[Source, String] ->
    case read(path) {
        Ok(kind) -> Ok(Source {kind: kind, path: None, url: Some("https://example.test")});
        Err(reason) -> Err(reason)
    }.
"#,
    )
    .expect("parse suspended typed-None record fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core])
        .expect("lower suspended typed-None record application");

    emit_native_application_object("suspended_result_option_record_source", &modules)
        .expect("emit suspended typed-None record object");
}

/// A managed function parameter remains available to the continuation created
/// for a suspending call in a later conditional arm.  Call-gate lowering must
/// carry the whole lexical scope into the selected arm; retaining only values
/// read by the gate condition loses aggregates needed after the suspension.
#[test]
fn gated_suspending_call_captures_a_managed_record_parameter() {
    let syntax = parse_module_as_syntax_output(
        r#"
module gated_managed_capture_source.

pub struct Options {
    path: String
}.

@compiler.native {probe.exists}
exists(path: String): Bool -> native.

validate(options: Options): Bool ->
    exists(options.path) and exists(options.path).

finish(options: Options, valid: Bool): Bool -> if {
    not valid -> false;
    true -> exists(options.path)
}.

pub evaluate(options: Options, enabled: Bool): Bool ->
    if {
        not enabled -> false;
        true ->
            let valid = validate(options);
            finish(options, valid)
    }.
"#,
    )
    .expect("parse gated managed-capture fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules =
        NativeModule::lower_application(&[&core]).expect("lower gated managed-capture application");

    emit_native_application_object("gated_managed_capture_source", &modules)
        .expect("emit gated managed-capture object");
}
