use super::test_support::{
    check_syntax_output_with_interface, check_syntax_output_with_std_interfaces,
};

/// Verifies resolved purity metadata takes precedence over familiar module names.
///
/// Inputs:
/// - A provider named `File` whose `exists/1` operation is explicitly pure.
/// - A pure caller that uses the operation both in a case guard and its body.
///
/// Output:
/// - Test passes without diagnostics.
///
/// Transformation:
/// - Prevents alias-only effect classification from treating unrelated user
///   modules as filesystem IO merely because their local alias is `File`.
#[test]
fn proven_pure_file_alias_is_not_classified_as_std_io() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module resolved_pure_file_alias.\n\
\n\
import provider.File.\n\
\n\
@pure\n\
pub available(path: String): Bool ->\n\
    case path {\n\
        value where File.exists(value) -> File.exists(value);\n\
        _ -> false\n\
    }.\n\
",
        "\
module provider.File.\n\
\n\
@pure\n\
pub exists(path: String): Bool.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies case guards consume resolved imported-effect facts.
///
/// Inputs:
/// - An imported provider operation without an explicit `@pure` proof.
/// - An ordinary function that calls the operation from a case guard.
///
/// Output:
/// - Test passes when the guard reports the stable imported-call diagnostic.
///
/// Transformation:
/// - Extends semantic effect checking to guards without relying on a fixed
///   list of standard-library aliases or operation names.
#[test]
fn case_guard_rejects_unproven_import_without_name_heuristics() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module resolved_impure_guard_import.\n\
\n\
import provider.External.\n\
\n\
pub classify(value: Int): Int ->\n\
    case value {\n\
        candidate where External.ready(candidate) -> candidate;\n\
        _ -> 0\n\
    }.\n\
",
        "\
module provider.External.\n\
\n\
pub ready(value: Int): Bool.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("case guard must be pure; found effectful imported function call")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies shipped native and VM interfaces use the general purity rule.
///
/// Inputs:
/// - Postgres connection, process spawn, and TCP listen operations.
/// - One `@pure` caller for each external capability family.
///
/// Output:
/// - Test passes when every caller reports an unproven imported effect.
///
/// Transformation:
/// - Proves database NativeBoundary calls, VM process state, and networking are
///   covered by resolved interface metadata rather than operation-name anchors.
#[test]
fn shipped_native_vm_and_network_calls_require_purity_proofs() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module resolved_shipped_effects.\n\
\n\
import std.db.Postgres.\n\
import std.vm.Process.\n\
import std.vm.Tcp.\n\
import type std.db.Postgres.{Config}.\n\
\n\
@pure\n\
pub open_database(config: Config): Dynamic ->\n\
    Postgres.connect(config).\n\
\n\
@pure\n\
pub start_process(): Dynamic ->\n\
    Process.spawn(\"worker\").\n\
\n\
@pure\n\
pub listen(): Dynamic ->\n\
    Tcp.listen(\"127.0.0.1:8080\").\n\
",
        "std/db/Postgres.terl",
    );

    for function_name in ["open_database", "start_process", "listen"] {
        let expected = format!(
            "function {function_name} annotated @pure must be pure; found effectful imported function call"
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(&expected)),
            "missing diagnostic `{expected}` in {:?}",
            diagnostics
        );
    }
}

/// Locks every roadmap-disallowed effect category to a stable diagnostic.
///
/// The provider deliberately exposes ordinary unproven interface functions:
/// purity is resolved from compiler metadata, never from operation spelling or
/// a trusted user promise. Each public wrapper represents one distinct external
/// capability family that an asserted-pure body must reject.
#[test]
fn disallowed_effect_taxonomy_reports_stable_purity_diagnostics() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.effect_taxonomy.\n\
\n\
import provider.Effects.\n\
\n\
@pure\n\
pub send_message(): Dynamic -> Effects.send_message().\n\
@pure\n\
pub mutate_vm_state(): Dynamic -> Effects.mutate_vm_state().\n\
@pure\n\
pub mutate_resource(): Dynamic -> Effects.mutate_resource().\n\
@pure\n\
pub call_native_boundary(): Dynamic -> Effects.call_native_boundary().\n\
@pure\n\
pub allocate_external_handle(): Dynamic -> Effects.allocate_external_handle().\n\
@pure\n\
pub read_clock(): Dynamic -> Effects.read_clock().\n\
@pure\n\
pub read_randomness(): Dynamic -> Effects.read_randomness().\n\
@pure\n\
pub start_process(): Dynamic -> Effects.start_process().\n\
@pure\n\
pub perform_io(): Dynamic -> Effects.perform_io().\n\
@pure\n\
pub query_database(): Dynamic -> Effects.query_database().\n\
",
        "\
module provider.Effects.\n\
\n\
pub send_message(): Dynamic.\n\
pub mutate_vm_state(): Dynamic.\n\
pub mutate_resource(): Dynamic.\n\
pub call_native_boundary(): Dynamic.\n\
pub allocate_external_handle(): Dynamic.\n\
pub read_clock(): Dynamic.\n\
pub read_randomness(): Dynamic.\n\
pub start_process(): Dynamic.\n\
pub perform_io(): Dynamic.\n\
pub query_database(): Dynamic.\n\
",
    );

    for function_name in [
        "send_message",
        "mutate_vm_state",
        "mutate_resource",
        "call_native_boundary",
        "allocate_external_handle",
        "read_clock",
        "read_randomness",
        "start_process",
        "perform_io",
        "query_database",
    ] {
        let expected = format!(
            "function {function_name} annotated @pure must be pure; found effectful imported function call"
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(&expected)),
            "missing diagnostic `{expected}` in {diagnostics:?}"
        );
    }
}
