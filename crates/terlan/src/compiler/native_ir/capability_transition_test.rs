//! Compiler-to-shard transition checks for declared asynchronous capabilities.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::lower_syntax_module_output_to_core;

use super::{emit_native_application_object, NativeExpr, NativeModule, NativeTransitionOperation};

fn lower(source: &str) -> NativeModule {
    lower_application(source).remove(0)
}

fn lower_application(source: &str) -> Vec<NativeModule> {
    let syntax = parse_module_as_syntax_output(source).expect("parse capability source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    NativeModule::lower_application(&[&core]).expect("lower capability application")
}

#[test]
fn declared_console_and_file_capabilities_suspend_with_typed_results() {
    let module = lower(
        "module native_capabilities.\n\n\
         pub print(): Unit -> std.io.Console.println(\"hello\").\n\n\
         pub exists(path: String): Bool -> std.io.File.exists(path).\n",
    );
    for name in ["print", "exists"] {
        let function = module
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("missing {name}"));
        assert!(matches!(
            function.body,
            NativeExpr::Suspend {
                operation: NativeTransitionOperation::Capability,
                ref arguments,
                ..
            } if matches!(arguments.first(), Some(NativeExpr::Int(1 | 2)))
                && arguments.len() == 5
        ));
    }
}

/// Verifies compiler-known SQL crosses the same asynchronous VM suspension
/// boundary as every other external capability, including exact parameter
/// type metadata recovered from the function's lexical scope.
#[test]
fn sql_query_suspends_with_typed_parameters_and_emits_native_object() {
    let modules = lower_application(
        r#"
module native_sql_capability.

pub type Option[T] = None | Some[T].
pub type Result[T, E] = Ok[T] | Err[E].

pub struct Error {
    message: String
}.

pub struct UserRow {
    id: String
}.

pub find(id: String): Result[Option[UserRow], Error] ->
    sql[UserRow] {SELECT id FROM users WHERE id = ${id} LIMIT 1}.
"#,
    );
    let function = modules[0]
        .functions
        .iter()
        .find(|function| function.name == "find")
        .expect("missing SQL function");
    assert!(matches!(
        function.body,
        NativeExpr::Suspend {
            operation: NativeTransitionOperation::Capability,
            ref arguments,
            ..
        } if matches!(arguments.first(), Some(NativeExpr::Int(42)))
            && arguments.len() == 15
    ));

    emit_native_application_object("native_sql_capability", &modules)
        .expect("emit typed SQL capability object");
}

/// Verifies call-profile construction waits for a later suspending callee
/// instead of caching an ABI-invalid ordinary call in the caller continuation.
#[test]
fn call_profile_fixed_point_waits_for_later_suspending_callee() {
    let modules = lower_application(
        "module native_capability_order.\n\n\
         pub a_caller(): Unit ->\n\
             let ignored = z_callee();\n\
             std.io.Console.println(\"done\").\n\n\
         pub z_callee(): Unit -> std.io.Console.println(\"callee\").\n",
    );

    emit_native_application_object("native_capability_order", &modules)
        .expect("emit caller after its suspending callee profile is available");
}

/// Verifies one bounded suspension profile can close over a tail-recursive
/// capability loop without allocating one continuation per iteration.
#[test]
fn tail_recursive_capability_loop_is_a_closed_aot_component() {
    let modules = lower_application(
        "module native_capability_loop.\n\n\
         pub visit(path: String, remaining: Int): Bool ->\n\
             let present = std.io.File.exists(path);\n\
             if {\n\
                 remaining == 0 -> present;\n\
                 true -> visit(path, remaining - 1)\n\
             }.\n",
    );

    emit_native_application_object("native_capability_loop", &modules)
        .expect("emit tail-recursive capability loop");
}

/// Verifies a source-recursive helper that becomes a synchronous native tail
/// loop can still complete through a caller-owned `CallThen` continuation.
#[test]
fn synchronous_native_tail_helper_completes_call_then() {
    let modules = lower_application(
        "module native_synchronous_tail_helper.\n\n\
         pub gather(remaining: Int, text: String): String ->\n\
             if {\n\
                 remaining == 0 -> text;\n\
                 true -> gather(remaining - 1, std.core.String.append(text, \"x\"))\n\
             }.\n\n\
         pub matches(): Bool -> gather(2, \"\") == \"xx\".\n",
    );

    emit_native_application_object("native_synchronous_tail_helper", &modules)
        .expect("emit synchronous tail helper caller");
}

/// A call chain contributes one completion frame per source call site.  It
/// must never clone every downstream suspension node into every caller.
#[test]
fn caller_completion_frames_keep_continuation_growth_linear() {
    const DEPTH: usize = 24;
    let mut source = String::from("module native_completion_frames.\n\n");
    for index in 0..DEPTH {
        source.push_str(&format!(
            "pub f{index}(): Unit -> let value = f{}(); value.\n\n",
            index + 1
        ));
    }
    source.push_str(&format!(
        "pub f{DEPTH}(): Unit -> std.io.Console.println(\"done\").\n"
    ));

    let modules = lower_application(&source);
    let continuation_count = modules
        .iter()
        .map(|module| module.continuations.len())
        .sum::<usize>();
    assert!(
        continuation_count <= DEPTH + 2,
        "{DEPTH} call sites produced {continuation_count} continuations"
    );
    emit_native_application_object("native_completion_frames", &modules)
        .expect("emit linear completion-frame chain");
}
