use super::*;
use crate::core_intrinsic_lowering::core_primitive_intrinsic;
use terlan_hir::resolve_syntax_module_output;
use terlan_syntax::parse_module_as_syntax_output;

/// Verifies primitive intrinsic calls have deterministic CoreIR contract text.
///
/// Inputs:
/// - None; constructs a typed `core.string.contains` intrinsic call
///   directly.
///
/// Output:
/// - Test passes when the intrinsic expression renders its registry key,
///   arguments, return type, effects, and span in stable contract text.
///
/// Transformation:
/// - Exercises the compiler-owned intrinsic CoreIR representation without
///   using backend module/function names.
#[test]
fn core_intrinsic_call_contract_text_is_backend_neutral() {
    let expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringContains),
        args: vec![
            CoreExpr::Binary("hello".to_string()),
            CoreExpr::Binary("ell".to_string()),
        ],
        return_type: CoreType::Bool,
        effects: CoreEffectSet {
            effects: vec!["pure".to_string()],
        },
        span: Span::new(3, 17),
    });

    assert_eq!(
            expr.contract_text(),
            "Intrinsic(core.string.contains;args=Binary(hello),Binary(ell);return=Bool;effects=Effects(pure);span=3:17))"
        );
}

/// Verifies implicit `type_of` lowers to a compiler-owned CoreIR intrinsic.
///
/// Inputs:
/// - A syntax-output module that calls `type_of(1)`.
///
/// Output:
/// - Test passes when the function body lowers to
///   `CoreExpr::Intrinsic(core.type.type_of)` with a named `Type` return.
///
/// Transformation:
/// - Parses normal Terlan source, typechecks the implicit prelude call, and
///   verifies CoreIR carries a backend-neutral intrinsic instead of an
///   ordinary local function call.
#[test]
fn syntax_output_lowering_to_core_maps_type_of_to_intrinsic() {
    let module = parse_module_as_syntax_output(
        "\
module core_type_of_intrinsic_boundary.\n\
\n\
pub demo(): Type ->\n\
    type_of(1).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "demo")
        .expect("core demo function");
    let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected type_of intrinsic, got {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TypeOf)
    );
    assert_eq!(call.args, vec![CoreExpr::Int(1)]);
    assert_eq!(call.return_type, CoreType::Named("Type".to_string()));
    assert_eq!(call.effects, core_pure_effect_set());
    assert!(
            core.contract_text().contains(
                "Intrinsic(core.type.type_of;args=Int(1);return=Named(Type);effects=Effects(pure);span="
            ),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies implicit `is_type` lowers to a compiler-owned CoreIR intrinsic.
///
/// Inputs:
/// - A syntax-output module that calls `is_type(1, Int)`.
///
/// Output:
/// - Test passes when the function body lowers to
///   `CoreExpr::Intrinsic(core.type.is_type)`.
///
/// Transformation:
/// - Parses source with an implicit type value, checks that `Int` has the
///   expression type `Type`, and verifies CoreIR preserves the comparison
///   as a backend-neutral intrinsic.
#[test]
fn syntax_output_lowering_to_core_maps_is_type_to_intrinsic() {
    let module = parse_module_as_syntax_output(
        "\
module core_is_type_intrinsic_boundary.\n\
\n\
pub demo(): Bool ->\n\
    is_type(1, Int).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "demo")
        .expect("core demo function");
    let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected is_type intrinsic, got {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IsType)
    );
    assert_eq!(
        call.args,
        vec![CoreExpr::Int(1), CoreExpr::Var("Int".to_string())]
    );
    assert_eq!(call.return_type, CoreType::Bool);
    assert_eq!(call.effects, core_pure_effect_set());
    assert!(
            core.contract_text().contains(
                "Intrinsic(core.type.is_type;args=Int(1),Var(Int);return=Bool;effects=Effects(pure);span="
            ),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies selected `std.core.String` calls lower to CoreIR intrinsics.
///
/// Inputs:
/// - A syntax-output module that calls `std.core.String.contains`.
///
/// Output:
/// - Test passes when the function body lowers to
///   `CoreExpr::Intrinsic(core.string.contains)` with typed string
///   arguments and a Bool return type.
///
/// Transformation:
/// - Parses normal Terlan source, lowers it through the CoreIR path, and
///   verifies the std.core primitive API call no longer appears as a
///   backend or ordinary remote call in CoreIR.
#[test]
fn syntax_output_lowering_to_core_maps_string_contains_to_intrinsic() {
    let module = parse_module_as_syntax_output(
        "\
module core_string_intrinsic_boundary.\n\
\n\
pub demo(): Bool ->\n\
    std.core.String.contains(\"hello\", \"ell\").\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "demo")
        .expect("core demo function");
    let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected string contains intrinsic, got {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringContains)
    );
    assert_eq!(
        call.args,
        vec![
            CoreExpr::Binary("\"hello\"".to_string()),
            CoreExpr::Binary("\"ell\"".to_string())
        ]
    );
    assert_eq!(call.return_type, CoreType::Bool);
    assert_eq!(call.effects, core_pure_effect_set());
    assert!(
            core.contract_text()
                .contains("Intrinsic(core.string.contains;args=Binary(\"hello\"),Binary(\"ell\");return=Bool;effects=Effects(pure);span="),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies primitive `Int.to_string` receiver calls lower to CoreIR intrinsics.
///
/// Inputs:
/// - A syntax-output module that calls `1.to_string()`.
///
/// Output:
/// - Test passes when the function body lowers to
///   `CoreExpr::Intrinsic(core.int.to_string)` with the integer receiver as
///   the first intrinsic argument.
///
/// Transformation:
/// - Parses receiver-method syntax, classifies the integer literal receiver
///   as the `std.core.Int` primitive owner, and lowers the call through the
///   same formal CoreIR intrinsic used by `std.core.Int.to_string(1)`.
#[test]
fn syntax_output_lowering_to_core_maps_int_receiver_to_string_to_intrinsic() {
    let module = parse_module_as_syntax_output(
        "\
module core_int_receiver_intrinsic_boundary.\n\
\n\
pub demo(): String ->\n\
    1.to_string().\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "demo")
        .expect("core demo function");
    let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected int to_string intrinsic, got {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IntToString)
    );
    assert_eq!(call.args, vec![CoreExpr::Int(1)]);
    assert_eq!(call.return_type, CoreType::String);
    assert_eq!(call.effects, core_pure_effect_set());
    assert!(
        core.contract_text().contains(
            "Intrinsic(core.int.to_string;args=Int(1);return=String;effects=Effects(pure);span="
        ),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies selected `std.io.Console` calls lower to CoreIR runtime capabilities.
///
/// Inputs:
/// - A syntax-output module that calls `std.io.Console.println`.
///
/// Output:
/// - Test passes when the function body lowers to
///   `CoreExpr::Intrinsic(runtime.console.println)` with one typed string
///   argument, a `Unit` return type, and an `io` effect label.
///
/// Transformation:
/// - Parses normal Terlan source, lowers it through the CoreIR path, and
///   verifies the std.io runtime API call no longer appears as a backend
///   or ordinary remote call in CoreIR.
#[test]
fn syntax_output_lowering_to_core_maps_console_println_to_runtime_capability() {
    let module = parse_module_as_syntax_output(
        "\
module core_console_runtime_boundary.\n\
\n\
pub demo(): Unit ->\n\
    std.io.Console.println(\"hello\").\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "demo")
        .expect("core demo function");
    let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected console println runtime capability, got {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::ConsolePrintln)
    );
    assert_eq!(call.args, vec![CoreExpr::Binary("\"hello\"".to_string())]);
    assert_eq!(call.return_type, CoreType::Named("Unit".to_string()));
    assert_eq!(call.effects, core_io_effect_set());
    assert!(
            core.contract_text().contains(
                "Intrinsic(runtime.console.println;args=Binary(\"hello\");return=Named(Unit);effects=Effects(io);span="
            ),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies all `std.log` calls lower to CoreIR runtime capabilities.
///
/// Inputs:
/// - Syntax-output modules that call each public `std.log` level helper.
///
/// Output:
/// - Test passes when every function body lowers to
///   `CoreExpr::Intrinsic(runtime.console.println)` with one typed string
///   argument, a `Unit` return type, and an `io` effect label.
///
/// Transformation:
/// - Parses normal Terlan source for each log level, lowers it through the
///   CoreIR path, and verifies the portable logging API calls do not remain
///   normal remote module calls that would require a generated `std_log`
///   runtime module.
#[test]
fn syntax_output_lowering_to_core_maps_all_std_log_levels_to_runtime_capability() {
    for level in ["debug", "info", "warn", "error"] {
        assert_std_log_level_lowers_to_runtime_capability(level);
    }
}

/// Asserts one `std.log` level lowers to the runtime console capability.
///
/// Inputs:
/// - `level`: public `std.log` function name to call.
///
/// Output:
/// - Test assertion success or panic.
///
/// Transformation:
/// - Builds a tiny source module for the selected level, lowers it to CoreIR,
///   and checks the resulting intrinsic call shape.
fn assert_std_log_level_lowers_to_runtime_capability(level: &str) {
    let module = parse_module_as_syntax_output(&format!(
        "\
module core_log_runtime_boundary.\n\
\n\
pub demo(): Unit ->\n\
    std.log.{level}(\"hello\").\n"
    ))
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "demo")
        .expect("core demo function");
    let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected std.log {level} runtime capability, got {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::ConsolePrintln)
    );
    assert_eq!(call.args, vec![CoreExpr::Binary("\"hello\"".to_string())]);
    assert_eq!(call.return_type, CoreType::Named("Unit".to_string()));
    assert_eq!(call.effects, core_io_effect_set());
}

/// Verifies selected `std.core.String` receiver methods lower to CoreIR intrinsics.
///
/// Inputs:
/// - A syntax-output module that calls `"hello".contains("ell")`.
///
/// Output:
/// - Test passes when the function body lowers to the same
///   `CoreExpr::Intrinsic(core.string.contains)` shape used by the
///   module-call spelling.
///
/// Transformation:
/// - Parses receiver-method source syntax, lowers it through the CoreIR
///   path, and verifies the receiver is prepended as the first intrinsic
///   argument so target backends only see backend-neutral primitive calls.
#[test]
fn syntax_output_lowering_to_core_maps_string_receiver_contains_to_intrinsic() {
    let module = parse_module_as_syntax_output(
        "\
module core_string_receiver_intrinsic_boundary.\n\
\n\
pub demo(): Bool ->\n\
    \"hello\".contains(\"ell\").\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "demo")
        .expect("core demo function");
    let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected string receiver contains intrinsic, got {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringContains)
    );
    assert_eq!(
        call.args,
        vec![
            CoreExpr::Binary("\"hello\"".to_string()),
            CoreExpr::Binary("\"ell\"".to_string())
        ]
    );
    assert_eq!(call.return_type, CoreType::Bool);
    assert_eq!(call.effects, core_pure_effect_set());
    assert!(
            core.contract_text()
                .contains("Intrinsic(core.string.contains;args=Binary(\"hello\"),Binary(\"ell\");return=Bool;effects=Effects(pure);span="),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies named primitive receiver arguments lower to CoreIR in ABI order.
///
/// Inputs:
/// - A receiver-style string `replace` call with `replacement` before
///   `pattern`.
///
/// Output:
/// - Test passes when CoreIR stores arguments as value, pattern, replacement.
///
/// Transformation:
/// - Reorders named primitive receiver method arguments through the shared
///   primitive parameter-name table before constructing the intrinsic CoreIR
///   call.
#[test]
fn syntax_output_lowering_to_core_reorders_primitive_receiver_named_arguments() {
    let module = parse_module_as_syntax_output(
        "\
module core_string_receiver_named_intrinsic_boundary.\n\
\n\
pub demo(): String ->\n\
    \"hello\".replace(replacement = \"x\", pattern = \"l\").\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "demo")
        .expect("core demo function");
    let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected string receiver replace intrinsic, got {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringReplace)
    );
    assert_eq!(
        call.args,
        vec![
            CoreExpr::Binary("\"hello\"".to_string()),
            CoreExpr::Binary("\"l\"".to_string()),
            CoreExpr::Binary("\"x\"".to_string())
        ]
    );
}

/// Verifies BEAM primitive stdlib calls resolve to CoreIR intrinsic ids.
///
/// Inputs:
/// - Source-level module/function/arity triples for `std.beam.Bytes`,
///   `std.beam.Timeout`, `std.beam.Tcp`, and `std.beam.Port`.
///
/// Output:
/// - Test passes when every selected operation maps to the expected closed
///   CoreIR primitive intrinsic.
///
/// Transformation:
/// - Exercises the typechecker intrinsic selector without relying on Erlang
///   backend names, keeping BEAM primitive capability admission centralized in
///   CoreIR.
#[test]
fn core_primitive_intrinsic_resolves_beam_library_primitives() {
    let cases = [
        (
            "std.beam.Bytes",
            "from_list",
            1,
            CorePrimitiveIntrinsic::BeamBytesFromList,
        ),
        (
            "std.beam.Bytes",
            "to_list",
            1,
            CorePrimitiveIntrinsic::BeamBytesToList,
        ),
        (
            "std.beam.Bytes",
            "length",
            1,
            CorePrimitiveIntrinsic::BeamBytesLength,
        ),
        (
            "std.beam.Bytes",
            "concat",
            2,
            CorePrimitiveIntrinsic::BeamBytesConcat,
        ),
        (
            "std.beam.Timeout",
            "milliseconds",
            1,
            CorePrimitiveIntrinsic::BeamTimeoutMilliseconds,
        ),
        (
            "std.beam.Timeout",
            "forever",
            0,
            CorePrimitiveIntrinsic::BeamTimeoutForever,
        ),
        (
            "std.beam.Tcp",
            "connect",
            3,
            CorePrimitiveIntrinsic::BeamTcpConnect,
        ),
        (
            "std.beam.Tcp",
            "send",
            2,
            CorePrimitiveIntrinsic::BeamTcpSend,
        ),
        (
            "std.beam.Tcp",
            "receive",
            3,
            CorePrimitiveIntrinsic::BeamTcpReceive,
        ),
        (
            "std.beam.Tcp",
            "close",
            1,
            CorePrimitiveIntrinsic::BeamTcpClose,
        ),
        (
            "std.beam.Port",
            "open",
            1,
            CorePrimitiveIntrinsic::BeamPortOpen,
        ),
        (
            "std.beam.Port",
            "write",
            2,
            CorePrimitiveIntrinsic::BeamPortWrite,
        ),
        (
            "std.beam.Port",
            "read",
            3,
            CorePrimitiveIntrinsic::BeamPortRead,
        ),
        (
            "std.beam.Port",
            "close",
            1,
            CorePrimitiveIntrinsic::BeamPortClose,
        ),
    ];

    for (module, function, arity, expected) in cases {
        assert_eq!(
            core_primitive_intrinsic(module, function, arity),
            Some(expected),
            "{module}.{function}/{arity} should resolve"
        );
    }
}

/// Verifies BEAM primitive registry keys stay stable.
///
/// Inputs:
/// - Newly admitted BEAM primitive intrinsic variants.
///
/// Output:
/// - Test passes when each variant serializes to its expected backend-neutral
///   registry key.
///
/// Transformation:
/// - Guards CoreIR contract text against accidental key churn for socket and
///   port primitives that future Terlan integration tests will depend on.
#[test]
fn beam_library_primitive_registry_keys_are_stable() {
    assert_eq!(
        CorePrimitiveIntrinsic::BeamBytesFromList.registry_key(),
        "beam.bytes.from_list"
    );
    assert_eq!(
        CorePrimitiveIntrinsic::BeamTimeoutForever.registry_key(),
        "beam.timeout.forever"
    );
    assert_eq!(
        CorePrimitiveIntrinsic::BeamTcpReceive.registry_key(),
        "beam.tcp.receive"
    );
    assert_eq!(
        CorePrimitiveIntrinsic::BeamPortOpen.registry_key(),
        "beam.port.open"
    );
}

/// Verifies BEAM primitive return-type metadata is available to CoreIR.
///
/// Inputs:
/// - Representative byte, timeout, TCP, and port intrinsic variants.
///
/// Output:
/// - Test passes when the return-type table exposes the source-level result
///   shapes used by type preservation and backend lowering.
///
/// Transformation:
/// - Exercises the new return-type rows without involving source parsing or a
///   specific target backend.
#[test]
fn beam_library_primitive_return_types_are_registered() {
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::BeamBytesLength),
        CoreType::Int
    );
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::BeamTimeoutForever),
        CoreType::Named("Timeout".to_string())
    );
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::BeamTcpConnect),
        CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("TcpSocket".to_string()),
                CoreType::Named("Error".to_string())
            ],
        }
    );
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::BeamPortRead),
        CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Bytes".to_string()),
                CoreType::Named("Error".to_string())
            ],
        }
    );
}

/// Verifies unsupported BEAM primitive arities are rejected by CoreIR selection.
///
/// Inputs:
/// - Known BEAM primitive names with deliberately wrong arities.
///
/// Output:
/// - Test passes when the selector returns `None`.
///
/// Transformation:
/// - Ensures capability admission remains exact so malformed source or stale
///   summaries cannot be silently lowered as target runtime operations.
#[test]
fn core_primitive_intrinsic_rejects_wrong_beam_primitive_arities() {
    assert_eq!(
        core_primitive_intrinsic("std.beam.Bytes", "from_list", 0),
        None
    );
    assert_eq!(
        core_primitive_intrinsic("std.beam.Timeout", "forever", 1),
        None
    );
    assert_eq!(core_primitive_intrinsic("std.beam.Tcp", "connect", 2), None);
    assert_eq!(core_primitive_intrinsic("std.beam.Port", "read", 2), None);
}
