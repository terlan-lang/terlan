use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
    Instruction, MemorySection, MemoryType, Module, TypeSection, ValType,
};

use super::*;
use crate::support::test_fs::temp_dir;

fn scalar(kind: &str, value: &str) -> WasmScalarArg {
    WasmScalarArg {
        kind: kind.to_string(),
        value: value.to_string(),
    }
}

fn write_manifest(path: &Path, checksum: String, exports: serde_json::Value) {
    let signatures = exports
        .as_array()
        .expect("exports array")
        .iter()
        .map(|export| WasmAbiSignature {
            name: export["name"].as_str().expect("export name").to_string(),
            params: export["params"]
                .as_array()
                .expect("export params")
                .iter()
                .map(|param| param["ty"].as_str().expect("param type").to_string())
                .collect(),
            result: export["result"]
                .as_str()
                .expect("export result")
                .to_string(),
        })
        .collect::<Vec<_>>();
    let manifest = json!({
        "schema_version": MANIFEST_SCHEMA,
        "artifact_kind": ARTIFACT_KIND,
        "compiler_version": "0.0.7",
        "target_profile": "wasm.core",
        "module": "wasm.RuntimeTest",
        "exports": exports,
        "validation_engine": "wasmparser",
        "abi_contract_checksum": wasm_abi_contract_checksum(),
        "signature_checksum": wasm_abi_signature_checksum(&signatures),
        "checksum": checksum,
    });
    fs::write(
        manifest_path(path),
        serde_json::to_string_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("write manifest");
}

fn write_artifact(name: &str, bytes: &[u8], exports: serde_json::Value) -> PathBuf {
    let root = temp_dir("wasm_runtime", name);
    let path = root.join("fixture.wasm");
    fs::write(&path, bytes).expect("write Wasm artifact");
    write_manifest(&path, wasm_checksum(bytes), exports);
    path
}

fn config(path: PathBuf, export: &str, args: Vec<WasmScalarArg>) -> WasmRunConfig {
    WasmRunConfig {
        artifact: path,
        export: Some(export.to_string()),
        args,
        host_returns: Vec::new(),
        expected: None,
        repeat: 3,
        timeout: Duration::from_secs(5),
    }
}

fn scalar_module() -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function([], [ValType::I64]);
    types.ty().function([], [ValType::F32]);
    types.ty().function([], [ValType::F64]);
    module.section(&types);

    let mut functions = FunctionSection::new();
    for type_index in 0..4 {
        functions.function(type_index);
    }
    module.section(&functions);

    let mut exports = ExportSection::new();
    for (index, name) in ["add", "wide", "single", "double"].into_iter().enumerate() {
        exports.export(name, ExportKind::Func, index as u32);
    }
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut add = Function::new([]);
    add.instruction(&Instruction::LocalGet(0));
    add.instruction(&Instruction::LocalGet(1));
    add.instruction(&Instruction::I32Add);
    add.instruction(&Instruction::End);
    code.function(&add);
    let mut wide = Function::new([]);
    wide.instruction(&Instruction::I64Const(9_007_199_254_740_991));
    wide.instruction(&Instruction::End);
    code.function(&wide);
    let mut single = Function::new([]);
    single.instruction(&Instruction::F32Const(1.5.into()));
    single.instruction(&Instruction::End);
    code.function(&single);
    let mut double = Function::new([]);
    double.instruction(&Instruction::F64Const(2.25.into()));
    double.instruction(&Instruction::End);
    code.function(&double);
    module.section(&code);
    module.finish()
}

fn scalar_exports() -> serde_json::Value {
    json!([
        {"name": "add", "params": [{"name": "left", "ty": "i32"}, {"name": "right", "ty": "i32"}], "result": "i32"},
        {"name": "wide", "params": [], "result": "i64"},
        {"name": "single", "params": [], "result": "f32"},
        {"name": "double", "params": [], "result": "f64"},
    ])
}

#[test]
fn parse_run_config_accepts_strict_scalar_arguments_and_limits() {
    let parsed = parse_run_config(&[
        "module.wasm".to_string(),
        "--export".to_string(),
        "add".to_string(),
        "--arg".to_string(),
        "i32:19".to_string(),
        "--repeat".to_string(),
        "4".to_string(),
        "--host-return".to_string(),
        "host.answer=i64:42".to_string(),
        "--expect".to_string(),
        "i32:42".to_string(),
        "--timeout-ms".to_string(),
        "250".to_string(),
    ])
    .expect("parse Wasm run config");

    assert_eq!(parsed.export.as_deref(), Some("add"));
    assert_eq!(parsed.args, vec![scalar("i32", "19")]);
    assert_eq!(
        parsed.host_returns,
        vec![WasmHostReturn {
            module: "host".to_string(),
            name: "answer".to_string(),
            value: scalar("i64", "42"),
        }]
    );
    assert_eq!(parsed.repeat, 4);
    assert_eq!(parsed.timeout, Duration::from_millis(250));
    assert_eq!(parsed.expected, Some(scalar("i32", "42")));
}

#[test]
fn parse_run_config_rejects_invalid_and_non_finite_scalars() {
    for invalid in ["u32:1", "i32:2147483648", "f32:NaN", "f64:inf"] {
        let message = parse_run_config(&[
            "module.wasm".to_string(),
            "--arg".to_string(),
            invalid.to_string(),
        ])
        .expect_err("invalid scalar must fail");
        assert!(message.starts_with("wasm-argument-invalid:"), "{message}");
    }
}

#[test]
fn execute_runs_all_supported_scalar_result_shapes_repeatedly() {
    let bytes = scalar_module();
    let path = write_artifact("scalar_results", &bytes, scalar_exports());

    assert_eq!(
        execute(&config(
            path.clone(),
            "add",
            vec![scalar("i32", "19"), scalar("i32", "23")]
        )),
        Ok("42".to_string())
    );
    assert_eq!(
        execute(&config(path.clone(), "wide", vec![])),
        Ok("9007199254740991".to_string())
    );
    assert_eq!(
        execute(&config(path.clone(), "single", vec![])),
        Ok("1.5".to_string())
    );
    assert_eq!(
        execute(&config(path, "double", vec![])),
        Ok("2.25".to_string())
    );
}

#[test]
fn execute_rejects_stale_manifest_and_argument_shape_before_runtime() {
    let bytes = scalar_module();
    let path = write_artifact("stale_manifest", &bytes, scalar_exports());
    write_manifest(
        &path,
        "fnv1a64:0000000000000000".to_string(),
        scalar_exports(),
    );
    let message = execute(&config(
        path.clone(),
        "add",
        vec![scalar("i32", "1"), scalar("i32", "2")],
    ))
    .expect_err("stale artifact must fail");
    assert!(message.starts_with("wasm-artifact-stale:"), "{message}");

    write_manifest(&path, wasm_checksum(&bytes), scalar_exports());
    let message = execute(&config(path, "add", vec![scalar("i64", "1")]))
        .expect_err("wrong argument shape must fail");
    assert!(message.starts_with("wasm-argument-invalid:"), "{message}");
}

#[test]
fn execute_rejects_stale_abi_namespace_and_signature_contracts() {
    let bytes = scalar_module();
    let path = write_artifact("stale_abi_contract", &bytes, scalar_exports());
    let manifest_file = manifest_path(&path);
    let mut manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_file).expect("read manifest"))
            .expect("parse manifest");
    manifest["abi_contract_checksum"] = json!("fnv1a64:0000000000000000");
    fs::write(
        &manifest_file,
        serde_json::to_string_pretty(&manifest).expect("serialize stale ABI manifest"),
    )
    .expect("write stale ABI manifest");

    let message =
        execute(&config(path.clone(), "wide", vec![])).expect_err("stale ABI namespace must fail");
    assert!(message.starts_with("wasm-abi-contract-stale:"), "{message}");

    write_manifest(&path, wasm_checksum(&bytes), scalar_exports());
    let mut manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_file).expect("read fresh manifest"))
            .expect("parse fresh manifest");
    manifest["signature_checksum"] = json!("fnv1a64:0000000000000000");
    fs::write(
        &manifest_file,
        serde_json::to_string_pretty(&manifest).expect("serialize stale signature manifest"),
    )
    .expect("write stale signature manifest");

    let message = execute(&config(path, "wide", vec![])).expect_err("stale signature must fail");
    assert!(
        message.starts_with("wasm-abi-signature-stale:"),
        "{message}"
    );
}

#[test]
fn execute_rejects_wrong_expected_result() {
    let bytes = scalar_module();
    let path = write_artifact("wrong_expected_result", &bytes, scalar_exports());
    let mut run = config(path, "add", vec![scalar("i32", "19"), scalar("i32", "23")]);
    run.expected = Some(scalar("i32", "41"));

    let message = execute(&run).expect_err("wrong expected value must fail");
    assert!(message.starts_with("wasm-result-mismatch:"), "{message}");
    assert!(
        message.contains("expected i32:41, received 42"),
        "{message}"
    );
}

#[test]
fn execute_reports_missing_export_and_import_families() {
    let bytes = scalar_module();
    let path = write_artifact("missing_export", &bytes, scalar_exports());
    let message = execute(&config(path, "absent", vec![])).expect_err("missing export must fail");
    assert!(message.starts_with("wasm-export-missing:"), "{message}");

    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import("host", "value", EntityType::Function(0));
    module.section(&imports);
    let imported = module.finish();
    let path = write_artifact(
        "missing_import",
        &imported,
        json!([{"name": "value", "params": [], "result": "i32"}]),
    );
    let message = execute(&config(path, "value", vec![])).expect_err("missing import must fail");
    assert!(message.starts_with("wasm-import-missing:"), "{message}");
    assert!(message.contains("wasm.RuntimeTest.value"), "{message}");
}

#[test]
fn execute_calls_typed_host_return_import() {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import("host", "value", EntityType::Function(0));
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut exports = ExportSection::new();
    exports.export("read_host", ExportKind::Func, 1);
    module.section(&exports);
    let mut function = Function::new([]);
    function.instruction(&Instruction::Call(0));
    function.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&function);
    module.section(&code);
    let bytes = module.finish();
    let path = write_artifact(
        "host_return",
        &bytes,
        json!([{"name": "read_host", "params": [], "result": "i32"}]),
    );
    let mut run = config(path, "read_host", vec![]);
    run.host_returns.push(WasmHostReturn {
        module: "host".to_string(),
        name: "value".to_string(),
        value: scalar("i32", "42"),
    });

    assert_eq!(execute(&run), Ok("42".to_string()));
}

#[test]
fn execute_rejects_memory_exports_before_invocation() {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 1,
        maximum: Some(1),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memories);
    let mut exports = ExportSection::new();
    exports.export("main", ExportKind::Func, 0);
    exports.export("memory", ExportKind::Memory, 0);
    module.section(&exports);
    let mut function = Function::new([]);
    function.instruction(&Instruction::I32Const(1));
    function.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&function);
    module.section(&code);
    let bytes = module.finish();
    let path = write_artifact(
        "memory_export",
        &bytes,
        json!([{"name": "main", "params": [], "result": "i32"}]),
    );

    let message =
        execute(&config(path, "main", vec![])).expect_err("memory export must be rejected");
    assert!(message.starts_with("wasm-export-unsupported:"), "{message}");
    assert!(message.contains("memory:memory"), "{message}");
}

#[test]
fn execute_reports_validation_trap_and_timeout_families() {
    let invalid_path = write_artifact(
        "invalid_stack_result",
        &single_function_module(&[]),
        json!([{"name": "main", "params": [], "result": "i32"}]),
    );
    let message =
        execute(&config(invalid_path, "main", vec![])).expect_err("invalid module must fail");
    assert!(message.starts_with("wasm-runtime-trap:"), "{message}");

    let trap_path = write_artifact(
        "runtime_trap",
        &single_function_module(&[Instruction::Unreachable]),
        json!([{"name": "main", "params": [], "result": "i32"}]),
    );
    let message = execute(&config(trap_path, "main", vec![])).expect_err("trap must fail");
    assert!(message.starts_with("wasm-runtime-trap:"), "{message}");

    let loop_path = write_artifact(
        "runtime_timeout",
        &single_function_module(&[
            Instruction::Loop(wasm_encoder::BlockType::Empty),
            Instruction::Br(0),
            Instruction::End,
            Instruction::Unreachable,
        ]),
        json!([{"name": "main", "params": [], "result": "i32"}]),
    );
    let mut timeout_config = config(loop_path, "main", vec![]);
    timeout_config.timeout = Duration::from_millis(100);
    let message = execute(&timeout_config).expect_err("loop must time out");
    assert!(message.starts_with("wasm-exec-timeout:"), "{message}");
}

fn single_function_module(instructions: &[Instruction<'_>]) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut exports = ExportSection::new();
    exports.export("main", ExportKind::Func, 0);
    module.section(&exports);
    let mut function = Function::new([]);
    for instruction in instructions {
        function.instruction(instruction);
    }
    function.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&function);
    module.section(&code);
    module.finish()
}
