use std::collections::BTreeMap;

use terlan_syntax::parse_module_as_syntax_output;

/// Verifies compiler-owned BEAM runtime calls still lower after method-call
/// syntax was added.
///
/// Inputs:
/// - A formal syntax-output module containing `erlang.integer_to_list(value)`.
///
/// Output:
/// - Test passes when direct Erlang lowering emits an Erlang remote call.
///
/// Transformation:
/// - Parses the source through canonical syntax output, where
///   `erlang.integer_to_list(...)` is method-shaped syntax, then verifies
///   the Erlang syntax bridge reclassifies the known backend runtime
///   root without enabling arbitrary receiver-method lowering.
#[test]
fn formal_syntax_output_direct_emit_lowers_known_backend_runtime_method_shape() {
    let module = parse_module_as_syntax_output(
        r#"
module backend_runtime_method_shape.

pub render(value: Int): String ->
erlang.integer_to_list(value).
"#,
    )
    .expect("parse backend runtime method-shaped call fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("formal subset should lower known backend runtime method shape")
    .render();

    assert!(
        output.contains("render(Value) ->\n    erlang:integer_to_list(Value)."),
        "output:\n{}",
        output
    );
}

/// Verifies the embedded SQL runtime boundary module is stable.
///
/// Inputs:
/// - No external inputs; reads the backend-owned runtime source string.
///
/// Output:
/// - Test passes when the runtime exports the wrapper functions used by SQL
///   lowering and delegates to the compiler-owned helper bridge.
///
/// Transformation:
/// - Locks the generated Erlang module name, exported arities, and explicit
///   private helper protocol without connecting to Postgres.
#[test]
fn embedded_sql_runtime_exports_wrapper_boundary_functions() {
    let source = super::emit_sql_runtime_to_erlang();

    assert!(source.contains("-module(terlan_sql_runtime)."));
    assert!(source.contains("-export([query_one/5, query/5, execute/5])."));
    assert!(source.contains("run_helper(<<\"query_one\">>, Sql, Params, Projection)"));
    assert!(source.contains("{spawn_executable, Helper}"));
    assert!(source.contains("TERLAN_SQL_RUNTIME_HELPER"));
    assert!(source.contains("row_record(RowType, Values)"));
    assert!(!source.contains("postgres_adapter_unavailable"));
}

/// Verifies the embedded native vector runtime owns opaque handle operations.
///
/// Inputs:
/// - No external inputs; reads the backend-owned runtime source string.
///
/// Output:
/// - Test passes when the runtime exports vector operations, allocates opaque
///   handles, and talks to the compiler-owned SafeNative helper.
///
/// Transformation:
/// - Locks the generated Erlang module name and handle ABI so BEAM modules can
///   retain references to native collection values without compiler-side list
///   lowering or ETS-backed vector storage.
#[test]
fn embedded_native_vector_runtime_exports_handle_bridge_functions() {
    let source = super::emit_native_vector_runtime_to_erlang();

    assert!(source.contains("-module(std_native_collections_vector_safe_native)."));
    assert!(source.contains(
        "-export([new/0, from_list/1, length/1, get_at/2, set_at/3, swap/3, push/2, to_list/1])."
    ));
    assert!(source.contains("-export_type([vector/1])."));
    assert!(source.contains(
        "-type vector(_T) :: {terlan_native_vector, non_neg_integer(), non_neg_integer()}."
    ));
    assert!(source.contains("{terlan_native_vector, Id, Generation}"));
    assert!(source.contains("__native-vector-runtime"));
    assert!(source.contains("TERLAN_NATIVE_VECTOR_RUNTIME_ALLOW_BEAM_FALLBACK"));
    assert!(source.contains("helper_call(Command)"));
    assert!(source.contains("term_to_binary(Value)"));
    assert!(source.contains("decode_term_result(Encoded)"));
    assert!(source.contains("native_vector_invalid_term"));
    assert!(source.contains("parse_handle_reply(Id, Generation)"));
    assert!(source.contains("native_vector_invalid_integer"));
    assert!(!source.contains("ets:"));
    assert!(!source.contains("terlan_native_vector_handles"));
    assert!(!source.contains("native_vector_adapter_unavailable"));
}
