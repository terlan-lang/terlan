use super::*;
use crate::backends::wasm::backend_ir::{
    WasmExport, WasmFunction, WasmFunctionBody, WasmInstruction, WasmModuleIr, WasmParam,
    WasmResultType,
};
use wasmparser::{ExternalKind, Operator, Parser, Payload, ValType};

/// Minimal decoded Wasm export shape used by emission smoke tests.
#[derive(Debug, PartialEq, Eq)]
struct WasmExportSmoke {
    params: Vec<ValType>,
    results: Vec<ValType>,
    ops: Vec<WasmOpSmoke>,
}

/// Stable subset of Wasm operators produced by the first Terlan Wasm slice.
#[derive(Debug, Clone, PartialEq, Eq)]
enum WasmOpSmoke {
    LocalGet(u32),
    I32Const(i32),
    I64Const(i64),
    F32ConstBits(u32),
    F64ConstBits(u64),
    I32Add,
    End,
}

/// Decodes one exported function from validated Wasm bytes.
///
/// Inputs:
/// - `bytes`: emitted Wasm module bytes.
/// - `export_name`: exported function to inspect.
///
/// Output:
/// - Function parameter/result ABI plus the operator stream.
///
/// Transformation:
/// - Uses `wasmparser` sections directly so the smoke test validates the final
///   binary boundary instead of only checking Terlan's internal backend IR.
fn decode_export_smoke(bytes: &[u8], export_name: &str) -> WasmExportSmoke {
    let mut function_types = Vec::new();
    let mut function_type_indices = Vec::new();
    let mut exported_function_index = None;
    let mut code_bodies = Vec::new();

    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("Wasm payload should parse") {
            Payload::TypeSection(reader) => {
                function_types = reader
                    .into_iter_err_on_gc_types()
                    .map(|function_type| function_type.expect("function type should parse"))
                    .collect();
            }
            Payload::FunctionSection(reader) => {
                function_type_indices = reader
                    .into_iter()
                    .map(|type_index| type_index.expect("function type index should parse"))
                    .collect();
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.expect("export should parse");
                    if export.name == export_name {
                        assert_eq!(export.kind, ExternalKind::Func);
                        exported_function_index = Some(export.index as usize);
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let ops = body
                    .get_operators_reader()
                    .expect("operators should parse")
                    .into_iter()
                    .map(|operator| match operator.expect("operator should parse") {
                        Operator::LocalGet { local_index } => WasmOpSmoke::LocalGet(local_index),
                        Operator::I32Const { value } => WasmOpSmoke::I32Const(value),
                        Operator::I64Const { value } => WasmOpSmoke::I64Const(value),
                        Operator::F32Const { value } => WasmOpSmoke::F32ConstBits(value.bits()),
                        Operator::F64Const { value } => WasmOpSmoke::F64ConstBits(value.bits()),
                        Operator::I32Add => WasmOpSmoke::I32Add,
                        Operator::End => WasmOpSmoke::End,
                        operator => panic!("unexpected operator in smoke test: {operator:?}"),
                    })
                    .collect::<Vec<_>>();
                code_bodies.push(ops);
            }
            _ => {}
        }
    }

    let function_index = exported_function_index.expect("exported function should exist");
    let type_index = *function_type_indices
        .get(function_index)
        .expect("function type index should exist") as usize;
    let function_type = function_types
        .get(type_index)
        .expect("function type should exist");

    WasmExportSmoke {
        params: function_type.params().to_vec(),
        results: function_type.results().to_vec(),
        ops: code_bodies
            .get(function_index)
            .expect("function body should exist")
            .clone(),
    }
}

#[test]
fn emit_module_validates_exported_i32_const_function() {
    let module_ir = WasmModuleIr::new(vec![WasmFunction::exported_i32_const("answer", 42)]);

    let bytes = emit_module(&module_ir).expect("module should emit");

    validate_module(&bytes).expect("module should validate");
}

#[test]
fn emit_module_smokes_exported_i32_add_binary_shape() {
    let module_ir = WasmModuleIr::new(vec![WasmFunction {
        name: "add".to_string(),
        params: vec![
            WasmParam {
                name: "a".to_string(),
                ty: WasmResultType::I32,
            },
            WasmParam {
                name: "b".to_string(),
                ty: WasmResultType::I32,
            },
        ],
        result: WasmResultType::I32,
        body: WasmFunctionBody::Instructions(vec![
            WasmInstruction::LocalGet(0),
            WasmInstruction::LocalGet(1),
            WasmInstruction::I32Add,
        ]),
        export: Some(WasmExport {
            name: "add".to_string(),
        }),
    }]);

    let bytes = emit_module(&module_ir).expect("module should emit");

    assert_eq!(
        decode_export_smoke(&bytes, "add"),
        WasmExportSmoke {
            params: vec![ValType::I32, ValType::I32],
            results: vec![ValType::I32],
            ops: vec![
                WasmOpSmoke::LocalGet(0),
                WasmOpSmoke::LocalGet(1),
                WasmOpSmoke::I32Add,
                WasmOpSmoke::End,
            ],
        }
    );
}

#[test]
fn emit_module_smokes_reserved_scalar_abi_binary_shapes() {
    let module_ir = WasmModuleIr::new(vec![
        WasmFunction {
            name: "wide".to_string(),
            params: Vec::new(),
            result: WasmResultType::I64,
            body: WasmFunctionBody::Instructions(vec![WasmInstruction::I64Const(
                9_223_372_036_854_775,
            )]),
            export: Some(WasmExport {
                name: "wide".to_string(),
            }),
        },
        WasmFunction {
            name: "single".to_string(),
            params: Vec::new(),
            result: WasmResultType::F32,
            body: WasmFunctionBody::Instructions(vec![WasmInstruction::F32ConstBits(
                1.5f32.to_bits(),
            )]),
            export: Some(WasmExport {
                name: "single".to_string(),
            }),
        },
        WasmFunction {
            name: "double".to_string(),
            params: Vec::new(),
            result: WasmResultType::F64,
            body: WasmFunctionBody::Instructions(vec![WasmInstruction::F64ConstBits(
                2.25f64.to_bits(),
            )]),
            export: Some(WasmExport {
                name: "double".to_string(),
            }),
        },
    ]);

    let bytes = emit_module(&module_ir).expect("module should emit");

    assert_eq!(
        decode_export_smoke(&bytes, "wide"),
        WasmExportSmoke {
            params: Vec::new(),
            results: vec![ValType::I64],
            ops: vec![
                WasmOpSmoke::I64Const(9_223_372_036_854_775),
                WasmOpSmoke::End,
            ],
        }
    );
    assert_eq!(
        decode_export_smoke(&bytes, "single"),
        WasmExportSmoke {
            params: Vec::new(),
            results: vec![ValType::F32],
            ops: vec![
                WasmOpSmoke::F32ConstBits(1.5f32.to_bits()),
                WasmOpSmoke::End,
            ],
        }
    );
    assert_eq!(
        decode_export_smoke(&bytes, "double"),
        WasmExportSmoke {
            params: Vec::new(),
            results: vec![ValType::F64],
            ops: vec![
                WasmOpSmoke::F64ConstBits(2.25f64.to_bits()),
                WasmOpSmoke::End,
            ],
        }
    );
}

#[test]
fn validate_module_rejects_invalid_bytes() {
    let err = validate_module(b"not wasm").expect_err("invalid bytes should fail");

    assert!(matches!(err, WasmEmitError::Validation(_)));
}

#[test]
fn emit_module_rejects_empty_module() {
    let err =
        emit_module(&WasmModuleIr::new(Vec::new())).expect_err("empty module should be rejected");

    assert_eq!(err, WasmEmitError::EmptyModule);
}
