use super::{WasmAbiType, WasmValue};

/// Verifies the Wasm ABI type spellings stay stable.
///
/// Inputs:
/// - Every scalar ABI type admitted by the initial Wasm backend.
///
/// Output:
/// - Canonical WebAssembly text spelling.
///
/// Transformation:
/// - Converts typed backend enum values into the strings used by future
///   signature metadata and diagnostics.
#[test]
fn wasm_abi_type_spelling_is_stable() {
    assert_eq!(WasmAbiType::I32.as_str(), "i32");
    assert_eq!(WasmAbiType::I64.as_str(), "i64");
    assert_eq!(WasmAbiType::F32.as_str(), "f32");
    assert_eq!(WasmAbiType::F64.as_str(), "f64");
}

/// Verifies scalar values report their ABI type.
///
/// Inputs:
/// - One value for each initial scalar ABI family.
///
/// Output:
/// - Matching `WasmAbiType` values.
///
/// Transformation:
/// - Classifies runtime-independent scalar payloads without inspecting the
///   payload value itself.
#[test]
fn wasm_value_reports_its_abi_type() {
    assert_eq!(WasmValue::I32(1).abi_type(), WasmAbiType::I32);
    assert_eq!(WasmValue::I64(1).abi_type(), WasmAbiType::I64);
    assert_eq!(WasmValue::F32(1.0).abi_type(), WasmAbiType::F32);
    assert_eq!(WasmValue::F64(1.0).abi_type(), WasmAbiType::F64);
}
