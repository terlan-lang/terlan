//! Source-to-object coverage for suspension-capable calls inside case branches.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::{emit_native_application_object, NativeModule};

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
