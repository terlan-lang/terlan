use super::*;

#[test]
fn abi_accepts_matching_scalar_result() {
    let abi = WasmFunctionAbi::new("answer", Vec::new(), vec![WasmAbiType::I32]);

    validate_export_result_value(&abi, Some(WasmValue::I32(42)))
        .expect("matching scalar result should validate");
}

#[test]
fn abi_rejects_mismatched_scalar_result() {
    let abi = WasmFunctionAbi::new("answer", Vec::new(), vec![WasmAbiType::I32]);

    let err = validate_export_result_value(&abi, Some(WasmValue::I64(42)))
        .expect_err("mismatched scalar result should fail");

    assert_eq!(
        err,
        WasmAbiError::ResultTypeMismatch {
            function: "answer".to_string(),
            expected: WasmAbiType::I32,
            actual: WasmAbiType::I64,
        }
    );
}

#[test]
fn abi_reserves_multi_value_results() {
    let abi = WasmFunctionAbi::new("pair", Vec::new(), vec![WasmAbiType::I32, WasmAbiType::I32]);

    let err = validate_export_result_value(&abi, Some(WasmValue::I32(1)))
        .expect_err("multi-value results should be reserved");

    assert_eq!(
        err,
        WasmAbiError::UnsupportedMultiValueResult {
            function: "pair".to_string(),
            count: 2,
        }
    );
}
