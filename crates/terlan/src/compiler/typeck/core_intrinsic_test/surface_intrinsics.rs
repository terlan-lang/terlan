use super::super::*;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;

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
pub(super) fn core_intrinsic_call_contract_text_is_backend_neutral() {
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
pub(super) fn syntax_output_lowering_to_core_maps_type_of_to_intrinsic() {
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
pub(super) fn syntax_output_lowering_to_core_maps_is_type_to_intrinsic() {
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
pub(super) fn syntax_output_lowering_to_core_maps_string_contains_to_intrinsic() {
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
pub(super) fn syntax_output_lowering_to_core_maps_int_receiver_to_string_to_intrinsic() {
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

/// Verifies local `String(value)` lowers to a generic to-string intrinsic.
///
/// Inputs:
/// - A syntax-output module that calls `String(1)`.
///
/// Output:
/// - Test passes when the function body lowers to
///   `CoreExpr::Intrinsic(core.value.to_string)` with the argument preserved.
///
/// Transformation:
/// - Treats `String(...)` as a compiler-owned conversion constructor rather
///   than an unresolved algebraic data constructor.
#[test]
pub(super) fn syntax_output_lowering_to_core_maps_string_constructor_to_value_to_string_intrinsic()
{
    let module = parse_module_as_syntax_output(
        "\
module core_string_constructor_intrinsic_boundary.\n\
\n\
pub demo(): String ->\n\
    String(1).\n",
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
            "expected value to_string intrinsic, got {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ValueToString)
    );
    assert_eq!(call.args, vec![CoreExpr::Int(1)]);
    assert_eq!(call.return_type, CoreType::String);
    assert_eq!(call.effects, core_pure_effect_set());
    assert!(
        core.contract_text().contains(
            "Intrinsic(core.value.to_string;args=Int(1);return=String;effects=Effects(pure);span="
        ),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies primitive constructors lower to `from_string` intrinsics.
///
/// Inputs:
/// - A syntax-output module that calls `Int("42")`, `Float("1.5")`, and
///   `Bool("true")`.
///
/// Output:
/// - Test passes when each constructor lowers to the matching
///   compiler-owned `core.*.from_string` intrinsic.
///
/// Transformation:
/// - Treats primitive constructor syntax as checked string parsing instead of
///   unresolved algebraic data constructor calls.
#[test]
pub(super) fn syntax_output_lowering_to_core_maps_primitive_constructors_to_from_string_intrinsics()
{
    let module = parse_module_as_syntax_output(
        "\
module core_primitive_constructor_intrinsic_boundary.\n\
\n\
pub int_demo(): Dynamic ->\n\
    Int(\"42\").\n\
\n\
pub float_demo(): Dynamic ->\n\
    Float(\"1.5\").\n\
\n\
pub bool_demo(): Dynamic ->\n\
    Bool(\"true\").\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let expected = [
        ("int_demo", CorePrimitiveIntrinsic::IntFromString),
        ("float_demo", CorePrimitiveIntrinsic::FloatFromString),
        ("bool_demo", CorePrimitiveIntrinsic::BoolFromString),
    ];
    for (function_name, intrinsic) in expected {
        let function = core
            .functions
            .iter()
            .find(|function| function.name == function_name)
            .expect("core function");
        let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
            panic!(
                "expected from_string intrinsic for {function_name}, got {:?}",
                function.clauses[0].body.core_expr
            );
        };
        assert_eq!(call.id, CoreIntrinsicId::Primitive(intrinsic));
    }
}

/// Verifies ordinary imported-module syntax reaches compiler-owned intrinsic
/// identity before native lowering.
#[test]
pub(super) fn syntax_output_lowering_canonicalizes_process_import_for_yield_intrinsic() {
    let module = parse_module_as_syntax_output(
        "module core_process_import.\n\nimport std.vm.Process.\n\npub demo(): Unit ->\n    Process.yield_now().\n",
    )
    .expect("parse imported Process fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let body = core
        .functions
        .iter()
        .find(|function| function.name == "demo")
        .and_then(|function| function.clauses.first())
        .and_then(|clause| clause.body.core_expr.as_ref())
        .expect("demo CoreIR body");
    assert!(matches!(
        body,
        CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessYield),
            args,
            ..
        }) if args.is_empty()
    ));
}

#[test]
pub(super) fn syntax_output_lowering_canonicalizes_process_send_int_transition() {
    let module = parse_module_as_syntax_output(
        "module core_process_send.\n\nimport std.vm.Process.\n\npub demo(recipient: Int, payload: Int): Unit ->\n    Process.send_int(recipient, payload).\n",
    )
    .expect("parse imported Process Send fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let body = core
        .functions
        .iter()
        .find(|function| function.name == "demo")
        .and_then(|function| function.clauses.first())
        .and_then(|clause| clause.body.core_expr.as_ref())
        .expect("demo CoreIR body");
    assert!(matches!(
        body,
        CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessSendInt),
            args,
            return_type: CoreType::Named(return_type),
            effects,
            ..
        }) if args.len() == 2
            && return_type == "Unit"
            && effects
                .effects
                .iter()
                .any(|effect| effect == "vm_effect_execution")
    ));
}

/// Verifies an unimported short module name cannot impersonate a compiler
/// intrinsic namespace.
#[test]
pub(super) fn syntax_output_lowering_does_not_promote_unimported_process_name() {
    let module = parse_module_as_syntax_output(
        "module core_process_unimported.\n\npub demo(): Unit ->\n    Process.yield_now().\n",
    )
    .expect("parse unimported Process fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let body = core
        .functions
        .iter()
        .find(|function| function.name == "demo")
        .and_then(|function| function.clauses.first())
        .and_then(|clause| clause.body.core_expr.as_ref())
        .expect("demo CoreIR body");
    assert!(matches!(
        body,
        CoreExpr::RemoteCall {
            module,
            function,
            args
        } if module == "Process" && function == "yield_now" && args.is_empty()
    ));
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
pub(super) fn syntax_output_lowering_to_core_maps_console_println_to_runtime_capability() {
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

/// Verifies portable stderr output retains its own CoreIR capability identity.
#[test]
pub(super) fn syntax_output_lowering_to_core_maps_console_eprintln_to_runtime_capability() {
    let module = parse_module_as_syntax_output(
        "\
module core_console_stderr_runtime_boundary.\n\
\n\
pub demo(): Unit ->\n\
    std.io.Console.eprintln(\"diagnostic\").\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse stderr fixture: {err:?}"));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let function = core
        .functions
        .iter()
        .find(|function| function.name == "demo")
        .expect("core stderr demo function");
    let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected console eprintln runtime capability, got {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::ConsoleEprintln)
    );
    assert_eq!(
        call.args,
        vec![CoreExpr::Binary("\"diagnostic\"".to_string())]
    );
    assert_eq!(call.return_type, CoreType::Named("Unit".to_string()));
    assert_eq!(call.effects, core_io_effect_set());
}

/// Verifies all `std.log.Log` calls lower to CoreIR runtime capabilities.
///
/// Inputs:
/// - Syntax-output modules that call each public `std.log.Log` level helper.
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
pub(super) fn syntax_output_lowering_to_core_maps_all_std_log_levels_to_runtime_capability() {
    for level in ["debug", "info", "warn", "error"] {
        assert_std_log_level_lowers_to_runtime_capability(level);
    }
}

/// Asserts one `std.log.Log` level lowers to the runtime console capability.
///
/// Inputs:
/// - `level`: public `std.log.Log` function name to call.
///
/// Output:
/// - Test assertion success or panic.
///
/// Transformation:
/// - Builds a tiny source module for the selected level, lowers it to CoreIR,
///   and checks the resulting intrinsic call shape.
pub(super) fn assert_std_log_level_lowers_to_runtime_capability(level: &str) {
    let module = parse_module_as_syntax_output(&format!(
        "\
module core_log_runtime_boundary.\n\
\n\
pub demo(): Unit ->\n\
    std.log.Log.{level}(\"hello\").\n"
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
            "expected std.log.Log {level} runtime capability, got {:?}",
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
pub(super) fn syntax_output_lowering_to_core_maps_string_receiver_contains_to_intrinsic() {
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
pub(super) fn syntax_output_lowering_to_core_reorders_primitive_receiver_named_arguments() {
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

/// Verifies `String.reverse` lowers to a backend-neutral primitive intrinsic.
///
/// Inputs:
/// - A receiver-style string `reverse` call.
///
/// Output:
/// - Test passes when CoreIR stores `core.string.reverse` instead of a remote
///   runtime call.
///
/// Transformation:
/// - Exercises the std.core.String receiver intrinsic registry so the API is
///   executable across VM and JS backends.
#[test]
pub(super) fn syntax_output_lowering_to_core_maps_string_reverse_to_intrinsic() {
    let module = parse_module_as_syntax_output(
        "\
module core_string_reverse_intrinsic_boundary.\n\
\n\
pub demo(): String ->\n\
    \"hello\".reverse().\n",
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
            "expected string receiver reverse intrinsic, got {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringReverse)
    );
    assert_eq!(call.args, vec![CoreExpr::Binary("\"hello\"".to_string())]);
}

/// Verifies `String.characters` lowers to the backend-neutral Unicode-scalar
/// list intrinsic rather than a backend-specific remote call.
#[test]
pub(super) fn syntax_output_lowering_to_core_maps_string_characters_to_intrinsic() {
    let module = parse_module_as_syntax_output(
        "\
module core_string_characters_intrinsic_boundary.\n\
\n\
pub demo(): List[String] ->\n\
    \"aé\".characters().\n",
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
            "expected string receiver characters intrinsic, got {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringCharacters)
    );
    assert_eq!(call.args, vec![CoreExpr::Binary("\"aé\"".to_string())]);
}

/// Verifies `String.codepoints` lowers to the compact Unicode-scalar integer
/// list intrinsic rather than allocating backend-specific character objects.
#[test]
pub(super) fn syntax_output_lowering_to_core_maps_string_codepoints_to_intrinsic() {
    let module = parse_module_as_syntax_output(
        "\
module core_string_codepoints_intrinsic_boundary.\n\
\n\
pub demo(): List[Int] ->\n\
    \"aé\".codepoints().\n",
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
            "expected string receiver codepoints intrinsic, got {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringCodepoints)
    );
    assert_eq!(call.return_type, CoreType::List(Box::new(CoreType::Int)));
    assert_eq!(call.args, vec![CoreExpr::Binary("\"aé\"".to_string())]);
}

#[test]
pub(super) fn syntax_output_lowering_to_core_maps_indexed_utf8_string_intrinsics() {
    let module = parse_module_as_syntax_output(
        "\
module core_string_indexed_utf8_intrinsic_boundary.\n\
\n\
pub byte_at(value: String): Int ->\n\
    std.core.String.utf8_byte_at(value, 1).\n\
\n\
pub slice(value: String): String ->\n\
    std.core.String.utf8_slice(value, 1, 2).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let calls = core
        .functions
        .iter()
        .map(|function| {
            let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
                panic!("expected indexed UTF-8 intrinsic for {}", function.name);
            };
            (function.name.as_str(), &call.id, &call.return_type)
        })
        .collect::<Vec<_>>();

    assert_eq!(
        calls,
        vec![
            (
                "byte_at",
                &CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringUtf8ByteAt),
                &CoreType::Int,
            ),
            (
                "slice",
                &CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringUtf8Slice),
                &CoreType::String,
            ),
        ]
    );
}
