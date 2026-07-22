use super::*;
use crate::backends::wasm::backend_ir::{WasmFunctionBody, WasmInstruction, WasmResultType};
use crate::backends::wasm::{emit_module, validate_module};
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::{ColorChoice, DiagnosticFormat};

/// Compiles a Terlan fixture through the formal pipeline.
///
/// Inputs:
/// - `source`: Terlan source for one implementation module.
///
/// Output:
/// - Checked CoreIR module.
///
/// Transformation:
/// - Reuses the production frontend so Wasm lowering tests consume the same
///   CoreIR shape as build, run, and VM execution paths.
fn compile_core(source: &str) -> crate::terlan_typeck::CoreModule {
    compile_core_with_profile(source, TargetProfile::default())
}

/// Compiles a Terlan fixture through the formal pipeline with a chosen profile.
///
/// Inputs:
/// - `source`: Terlan source for one implementation module.
/// - `profile`: target profile used by target-specific import gates.
///
/// Output:
/// - Checked CoreIR module.
///
/// Transformation:
/// - Reuses the production frontend while allowing Wasm ABI import tests to
///   select the Wasm profile without widening normal lowerer diagnostics.
fn compile_core_with_profile(
    source: &str,
    profile: TargetProfile,
) -> crate::terlan_typeck::CoreModule {
    crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "wasm_lowering_test.terl",
        source,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        None,
        NativePolicy::NativeBoundaryOptional,
        profile,
    )
    .expect("compile Wasm lowering fixture")
    .core
}

/// Returns a lowered function body by export name.
///
/// Inputs:
/// - `module_ir`: lowered Wasm module IR.
/// - `name`: exported function name.
///
/// Output:
/// - Borrowed Wasm function body.
///
/// Transformation:
/// - Keeps multi-export tests independent of declaration ordering.
fn body_for<'a>(
    module_ir: &'a crate::backends::wasm::WasmModuleIr,
    name: &str,
) -> &'a WasmFunctionBody {
    &module_ir
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("missing lowered function `{name}`"))
        .body
}

/// Proves typed constants are substituted before Wasm lowering and emission.
#[test]
fn value_lifecycle_constants_are_substituted_for_wasm() {
    let core = compile_core_with_profile(
        r#"
module wasm.ValueLifecycle.

pub const ANSWER: Int = 40 + 2.

pub answer(): Int -> ANSWER.
"#,
        TargetProfile::WasmCore,
    );
    let module_ir = lower_core_module(&core).expect("lower substituted constant to Wasm IR");
    assert_eq!(
        body_for(&module_ir, "answer"),
        &WasmFunctionBody::Instructions(vec![WasmInstruction::I32Const(42)])
    );
    let bytes = emit_module(&module_ir).expect("emit Wasm bytes");
    validate_module(&bytes).expect("validate emitted Wasm bytes");
    assert!(!String::from_utf8_lossy(&bytes).contains("ANSWER"));
}

/// Asserts one typed CoreIR fixture is rejected as an unsupported Wasm body.
///
/// Inputs:
/// - `source`: Terlan source that typechecks but is outside the first Wasm
///   lowering subset.
/// - `function`: public export expected to fail lowering.
/// - `body`: stable CoreIR body-kind diagnostic.
///
/// Output: assertion success.
/// Transformation: keeps unsupported-CoreIR coverage on the real frontend
/// while avoiding byte emission.
fn assert_unsupported_body(source: &str, function: &str, body: &str) {
    let core = compile_core(source);

    let err = lower_core_module(&core).expect_err("body lowering should be unsupported");

    assert_eq!(
        err,
        WasmLowerError::UnsupportedBody {
            function: function.to_string(),
            body: body.to_string()
        }
    );
}

/// Verifies checked CoreIR can lower into validated Wasm backend IR.
///
/// Inputs:
/// - A public zero-arity Terlan function returning an `Int` literal.
///
/// Output:
/// - Exported Wasm function IR and validated Wasm bytes.
///
/// Transformation:
/// - Exercises the real frontend to CoreIR, lowers the supported subset into
///   Wasm IR, emits bytes through `wasm-encoder`, and validates them through
///   `wasmparser`.
#[test]
fn lower_core_module_exports_zero_arity_int_constant() {
    let core = compile_core(
        r#"
module wasm.Const.

pub answer(): Int ->
    42.
"#,
    );

    let module_ir = lower_core_module(&core).expect("lower CoreIR to Wasm IR");

    assert_eq!(module_ir.functions.len(), 1);
    let function = &module_ir.functions[0];
    assert_eq!(function.name, "answer");
    assert_eq!(function.result, WasmResultType::I32);
    assert_eq!(
        function.body,
        WasmFunctionBody::Instructions(vec![WasmInstruction::I32Const(42)])
    );
    assert_eq!(
        function.export.as_ref().map(|export| export.name.as_str()),
        Some("answer")
    );

    let bytes = emit_module(&module_ir).expect("emit Wasm bytes");
    validate_module(&bytes).expect("validate emitted Wasm bytes");
}

/// Verifies checked CoreIR arithmetic lowers into Wasm stack instructions.
///
/// Inputs:
/// - A public zero-arity Terlan function returning `40 + 2 * 3 - 1`.
///
/// Output:
/// - Wasm instruction stream preserving Terlan arithmetic structure.
///
/// Transformation:
/// - Exercises recursive CoreIR lowering for the first supported arithmetic
///   operators without constant-folding away the stack operations.
#[test]
fn lower_core_module_exports_zero_arity_int_arithmetic() {
    let core = compile_core(
        r#"
module wasm.Arithmetic.

pub answer(): Int ->
    40 + 2 * 3 - 1.
"#,
    );

    let module_ir = lower_core_module(&core).expect("lower arithmetic CoreIR to Wasm IR");

    assert_eq!(
        module_ir.functions[0].body,
        WasmFunctionBody::Instructions(vec![
            WasmInstruction::I32Const(40),
            WasmInstruction::I32Const(2),
            WasmInstruction::I32Const(3),
            WasmInstruction::I32Mul,
            WasmInstruction::I32Add,
            WasmInstruction::I32Const(1),
            WasmInstruction::I32Sub,
        ])
    );

    let bytes = emit_module(&module_ir).expect("emit Wasm arithmetic bytes");
    validate_module(&bytes).expect("validate emitted Wasm arithmetic bytes");
}

/// Verifies checked CoreIR comparisons lower into Wasm i32 comparison ops.
///
/// Inputs:
/// - Public Terlan functions returning `Bool` from integer comparisons.
///
/// Output:
/// - Wasm comparison instructions whose result is represented as i32.
///
/// Transformation:
/// - Locks the first boolean result ABI as Wasm i32 and covers each supported
///   comparison operator before CLI Wasm target promotion.
#[test]
fn lower_core_module_exports_int_comparisons_as_i32_bools() {
    let core = compile_core(
        r#"
module wasm.Compare.

pub eq(a: Int, b: Int): Bool -> a == b.
pub ne(a: Int, b: Int): Bool -> a != b.
pub lt(a: Int, b: Int): Bool -> a < b.
pub lte(a: Int, b: Int): Bool -> a <= b.
pub gt(a: Int, b: Int): Bool -> a > b.
pub gte(a: Int, b: Int): Bool -> a >= b.
"#,
    );

    let module_ir = lower_core_module(&core).expect("lower comparison CoreIR to Wasm IR");

    assert_eq!(
        body_for(&module_ir, "eq"),
        &WasmFunctionBody::Instructions(vec![
            WasmInstruction::LocalGet(0),
            WasmInstruction::LocalGet(1),
            WasmInstruction::I32Eq,
        ])
    );
    assert_eq!(
        body_for(&module_ir, "ne"),
        &WasmFunctionBody::Instructions(vec![
            WasmInstruction::LocalGet(0),
            WasmInstruction::LocalGet(1),
            WasmInstruction::I32Ne,
        ])
    );
    assert_eq!(
        body_for(&module_ir, "lt"),
        &WasmFunctionBody::Instructions(vec![
            WasmInstruction::LocalGet(0),
            WasmInstruction::LocalGet(1),
            WasmInstruction::I32LtS,
        ])
    );
    assert_eq!(
        body_for(&module_ir, "lte"),
        &WasmFunctionBody::Instructions(vec![
            WasmInstruction::LocalGet(0),
            WasmInstruction::LocalGet(1),
            WasmInstruction::I32LeS,
        ])
    );
    assert_eq!(
        body_for(&module_ir, "gt"),
        &WasmFunctionBody::Instructions(vec![
            WasmInstruction::LocalGet(0),
            WasmInstruction::LocalGet(1),
            WasmInstruction::I32GtS,
        ])
    );
    assert_eq!(
        body_for(&module_ir, "gte"),
        &WasmFunctionBody::Instructions(vec![
            WasmInstruction::LocalGet(0),
            WasmInstruction::LocalGet(1),
            WasmInstruction::I32GeS,
        ])
    );

    let bytes = emit_module(&module_ir).expect("emit Wasm comparison bytes");
    validate_module(&bytes).expect("validate emitted Wasm comparison bytes");
}

/// Verifies unsupported CoreIR bodies fail before Wasm emission.
///
/// Inputs:
/// - A public zero-arity Terlan function whose body is a `let` expression.
///
/// Output:
/// - Stable unsupported-body lowering error.
///
/// Transformation:
/// - Keeps the first lowering slice honest by preventing the emitter from
///   accepting CoreIR expressions it cannot yet encode.
#[test]
fn lower_core_module_rejects_unsupported_body_before_emission() {
    let core = compile_core(
        r#"
module wasm.Unsupported.

pub answer(): Int ->
    let value = 42;
    value.
"#,
    );

    let err = lower_core_module(&core).expect_err("let lowering is not implemented yet");

    assert_eq!(
        err,
        WasmLowerError::UnsupportedBody {
            function: "answer".to_string(),
            body: "Let".to_string()
        }
    );
}

/// Verifies typed but unsupported CoreIR forms fail before Wasm emission.
///
/// Inputs:
/// - Public Terlan functions using `case`, same-module calls, and unary
///   operators while still returning `Int`.
///
/// Output:
/// - Stable unsupported-body diagnostics for each CoreIR form.
///
/// Transformation:
/// - Broadens the unsupported subset gate so additional typed CoreIR forms do
///   not reach byte emission accidentally.
#[test]
fn lower_core_module_rejects_typed_unsupported_core_forms_before_emission() {
    assert_unsupported_body(
        r#"
module wasm.UnsupportedCase.

pub answer(value: Int): Int ->
    case value {
        1 -> 42;
        _ -> 0
    }.
"#,
        "answer",
        "Case",
    );

    assert_unsupported_body(
        r#"
module wasm.UnsupportedCall.

pub answer(value: Int): Int ->
    helper(value).

helper(value: Int): Int ->
    value.
"#,
        "answer",
        "Call",
    );

    assert_unsupported_body(
        r#"
module wasm.UnsupportedUnary.

pub answer(value: Int): Int ->
    -value.
"#,
        "answer",
        "UnaryOp",
    );
}

/// Verifies exported `Int` parameters lower into Wasm locals.
///
/// Inputs:
/// - A public Terlan function with two typed `Int` parameters.
///
/// Output:
/// - Wasm parameter metadata and `local.get` instructions.
///
/// Transformation:
/// - Proves the first parameter ABI slice consumes checked CoreIR parameter
///   metadata and lowers variable references without source reparsing.
#[test]
fn lower_core_module_exports_int_parameters_as_locals() {
    let core = compile_core(
        r#"
module wasm.Param.

pub add(a: Int, b: Int): Int ->
    a + b * 2.
"#,
    );

    let module_ir = lower_core_module(&core).expect("lower parameterized CoreIR to Wasm IR");

    assert_eq!(module_ir.functions[0].params.len(), 2);
    assert_eq!(module_ir.functions[0].params[0].name, "a");
    assert_eq!(module_ir.functions[0].params[0].ty, WasmResultType::I32);
    assert_eq!(module_ir.functions[0].params[1].name, "b");
    assert_eq!(module_ir.functions[0].params[1].ty, WasmResultType::I32);
    assert_eq!(
        module_ir.functions[0].body,
        WasmFunctionBody::Instructions(vec![
            WasmInstruction::LocalGet(0),
            WasmInstruction::LocalGet(1),
            WasmInstruction::I32Const(2),
            WasmInstruction::I32Mul,
            WasmInstruction::I32Add,
        ])
    );

    let bytes = emit_module(&module_ir).expect("emit Wasm parameter bytes");
    validate_module(&bytes).expect("validate emitted Wasm parameter bytes");
}

/// Verifies explicit `std.wasm.Abi.I32` aliases lower to Wasm i32 params/results.
///
/// Inputs:
/// - A public Terlan function importing `I32` from `std.wasm.Abi`.
///
/// Output:
/// - Wasm parameter metadata and arithmetic instructions using i32.
///
/// Transformation:
/// - Proves the source-level ABI alias stays ordinary Terlan type syntax while
///   the Wasm backend recognizes it as the same scalar boundary type.
#[test]
fn lower_core_module_exports_wasm_i32_abi_aliases_as_locals() {
    let core = compile_core_with_profile(
        r#"
module wasm.Param.

import std.wasm.Abi.{I32}.

pub add(a: I32, b: I32): I32 ->
    a + b.
"#,
        TargetProfile::WasmCore,
    );

    let module_ir = lower_core_module(&core).expect("lower I32 parameterized CoreIR to Wasm IR");

    assert_eq!(module_ir.functions[0].params.len(), 2);
    assert_eq!(module_ir.functions[0].params[0].name, "a");
    assert_eq!(module_ir.functions[0].params[0].ty, WasmResultType::I32);
    assert_eq!(module_ir.functions[0].params[1].name, "b");
    assert_eq!(module_ir.functions[0].params[1].ty, WasmResultType::I32);
    assert_eq!(
        module_ir.functions[0].body,
        WasmFunctionBody::Instructions(vec![
            WasmInstruction::LocalGet(0),
            WasmInstruction::LocalGet(1),
            WasmInstruction::I32Add,
        ])
    );

    let bytes = emit_module(&module_ir).expect("emit Wasm I32 ABI bytes");
    validate_module(&bytes).expect("validate emitted Wasm I32 ABI bytes");
}

/// Verifies every public `std.wasm.Abi` scalar survives source lowering.
#[test]
fn lower_core_module_exports_all_wasm_scalar_aliases() {
    let core = compile_core_with_profile(
        r#"
module wasm.Scalars.

import std.wasm.Abi.{F32, F64, I32, I64}.

pub identity_i32(value: I32): I32 -> value.
pub identity_i64(value: I64): I64 -> value.
pub identity_f32(value: F32): F32 -> value.
pub identity_f64(value: F64): F64 -> value.
pub wide(): I64 -> 9007199254740991.
pub single(): F32 -> 1.5.
pub double(): F64 -> 2.25.
"#,
        TargetProfile::WasmCore,
    );

    let module_ir = lower_core_module(&core).expect("lower scalar ABI CoreIR");
    let signatures = module_ir
        .functions
        .iter()
        .map(|function| {
            (
                function.name.as_str(),
                function
                    .params
                    .iter()
                    .map(|param| param.ty)
                    .collect::<Vec<_>>(),
                function.result,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        signatures,
        vec![
            ("double", vec![], WasmResultType::F64),
            (
                "identity_f32",
                vec![WasmResultType::F32],
                WasmResultType::F32
            ),
            (
                "identity_f64",
                vec![WasmResultType::F64],
                WasmResultType::F64
            ),
            (
                "identity_i32",
                vec![WasmResultType::I32],
                WasmResultType::I32
            ),
            (
                "identity_i64",
                vec![WasmResultType::I64],
                WasmResultType::I64
            ),
            ("single", vec![], WasmResultType::F32),
            ("wide", vec![], WasmResultType::I64),
        ]
    );
    assert_eq!(
        module_ir.functions[0].body,
        WasmFunctionBody::Instructions(vec![WasmInstruction::F64ConstBits(2.25_f64.to_bits())])
    );
    assert_eq!(
        module_ir.functions[5].body,
        WasmFunctionBody::Instructions(vec![WasmInstruction::F32ConstBits(1.5_f32.to_bits())])
    );
    assert_eq!(
        module_ir.functions[6].body,
        WasmFunctionBody::Instructions(vec![WasmInstruction::I64Const(9_007_199_254_740_991)])
    );

    let bytes = emit_module(&module_ir).expect("emit scalar ABI bytes");
    validate_module(&bytes).expect("validate scalar ABI bytes");
}

/// Verifies non-`Int` exported parameters are rejected before emission.
///
/// Inputs:
/// - A public Terlan function with one `Binary` parameter.
///
/// Output:
/// - Stable unsupported-parameter-type lowering error.
///
/// Transformation:
/// - Keeps the first parameter ABI slice narrow until richer Wasm ABI types
///   are wired through `std.wasm.Abi`.
#[test]
fn lower_core_module_rejects_non_int_parameters() {
    let core = compile_core(
        r#"
module wasm.Param.

pub size(value: Binary): Int ->
    1.
"#,
    );

    let err = lower_core_module(&core).expect_err("Binary parameter lowering is not implemented");

    assert_eq!(
        err,
        WasmLowerError::UnsupportedParamType {
            function: "size".to_string(),
            param: "value".to_string(),
            param_type: "Binary".to_string()
        }
    );
}
