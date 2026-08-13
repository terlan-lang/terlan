//! Tests for maintained accelerator AOT backend contracts.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    compiler::accelerator::{
        accelerator_toolchain_sha256, AcceleratorExecutionDimensions, AcceleratorIrAccess,
        AcceleratorIrAddressSpace, AcceleratorIrKernel, AcceleratorIrNode, AcceleratorIrOperation,
        AcceleratorIrParameter, AcceleratorIrType, AcceleratorKernelSelection,
        AcceleratorScalarType, ACCELERATOR_IR_SCHEMA,
    },
    terlan_hir::resolve_syntax_module_output,
    terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output},
};

use super::*;

/// Returns one unique test output directory under the workspace target tree.
fn output_directory(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/quality/accelerator-aot-test")
        .join(format!("{name}-{}-{nonce}", std::process::id()))
}

/// Produces one checked scalar branch kernel.
fn accelerator_ir() -> AcceleratorIrModule {
    let syntax = parse_module_as_syntax_output(
        "module aot_fixture.\n\
         pub choose(left: Int, right: Int): Int ->\n\
             if { left > right -> left + 2; true -> right * 3 }.\n",
    )
    .expect("parse AOT fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    AcceleratorIrModule::lower(
        &core,
        &[AcceleratorKernelSelection {
            function: "choose".to_string(),
            specializations: BTreeMap::new(),
            buffer_parameters: BTreeMap::new(),
            dimensions: AcceleratorExecutionDimensions {
                grid: [1, 1, 1],
                block: [32, 1, 1],
            },
            shared_memory_bytes: 0,
            synchronization_points: Vec::new(),
            math_operations: BTreeSet::new(),
            source: AcceleratorIrSource {
                file: "aot_fixture.terl".to_string(),
                line: 2,
                column: 1,
            },
        }],
    )
    .expect("lower AOT fixture")
}

/// Produces one typed device-buffer copy kernel.
fn buffer_ir() -> AcceleratorIrModule {
    let source = AcceleratorIrSource {
        file: "buffer_fixture.terl".to_string(),
        line: 3,
        column: 1,
    };
    let scalar = AcceleratorIrType::Scalar {
        dtype: AcceleratorScalarType::F32,
    };
    let index = AcceleratorIrNode {
        ty: AcceleratorIrType::Scalar {
            dtype: AcceleratorScalarType::I64,
        },
        source: source.clone(),
        operation: AcceleratorIrOperation::Int { value: 0 },
    };
    let load = AcceleratorIrNode {
        ty: scalar,
        source: source.clone(),
        operation: AcceleratorIrOperation::Load {
            buffer: "input".to_string(),
            index: Box::new(index.clone()),
        },
    };
    AcceleratorIrModule {
        schema: ACCELERATOR_IR_SCHEMA,
        module: "buffer_fixture".to_string(),
        kernels: vec![AcceleratorIrKernel {
            name: "copy_first".to_string(),
            core_identity: "buffer_fixture.copy_first/2".to_string(),
            specializations: BTreeMap::new(),
            parameters: vec![
                AcceleratorIrParameter {
                    name: "output".to_string(),
                    ty: AcceleratorIrType::Buffer {
                        dtype: AcceleratorScalarType::F32,
                        address_space: AcceleratorIrAddressSpace::Device,
                        access: AcceleratorIrAccess::Write,
                        alignment: 4,
                        alias_class: 1,
                    },
                },
                AcceleratorIrParameter {
                    name: "input".to_string(),
                    ty: AcceleratorIrType::Buffer {
                        dtype: AcceleratorScalarType::F32,
                        address_space: AcceleratorIrAddressSpace::Device,
                        access: AcceleratorIrAccess::Read,
                        alignment: 4,
                        alias_class: 2,
                    },
                },
            ],
            return_type: AcceleratorIrType::Unit,
            dimensions: AcceleratorExecutionDimensions {
                grid: [1, 1, 1],
                block: [1, 1, 1],
            },
            shared_memory_bytes: 0,
            synchronization_points: Vec::new(),
            body: AcceleratorIrNode {
                ty: AcceleratorIrType::Unit,
                source: source.clone(),
                operation: AcceleratorIrOperation::Store {
                    buffer: "output".to_string(),
                    index: Box::new(index),
                    value: Box::new(load),
                },
            },
            source,
        }],
    }
}

/// Creates an admitted LLVM toolchain from one explicit executable.
fn llvm_toolchain(executable: &Path) -> AcceleratorAdmittedToolchain {
    AcceleratorAdmittedToolchain {
        name: "llvm-nvptx".to_string(),
        version: "14.0.0".to_string(),
        executable: executable.to_string_lossy().into_owned(),
        executable_sha256: accelerator_toolchain_sha256(executable).expect("hash LLVM toolchain"),
        license: "Apache-2.0 WITH LLVM-exception".to_string(),
    }
}

#[test]
fn aot_errors_have_stable_diagnostic_codes() {
    assert_eq!(
        AcceleratorAotError::InvalidArtifact("fixture".to_string()).code(),
        "accelerator.aot-invalid-artifact"
    );
}

#[test]
fn llvm_nvptx_compiles_validates_caches_and_reproduces_ptx() {
    let executable = Path::new("/usr/bin/llc");
    if !executable.is_file() {
        return;
    }
    let ir = accelerator_ir();
    let toolchain = llvm_toolchain(executable);
    let first_directory = output_directory("first");
    let second_directory = output_directory("second");
    let backend = LlvmNvptxBackend;
    let compile = |directory: &Path| {
        backend.compile(&AcceleratorAotRequest {
            ir: &ir,
            architecture: "sm-30",
            toolchain: &toolchain,
            build_options: BTreeMap::from([("optimization".to_string(), "2".to_string())]),
            output_directory: directory,
        })
    };

    let first = compile(&first_directory).expect("compile first PTX artifact");
    assert!(!first.cache_hit);
    assert_eq!(first.descriptor.kernels[0].entrypoint, "choose");
    assert_eq!(first.descriptor.sources[0].source.line, 2);
    assert!(String::from_utf8_lossy(&first.bytes).contains(".visible .entry choose("));
    let cached = compile(&first_directory).expect("restore cached PTX artifact");
    assert!(cached.cache_hit);
    assert_eq!(first.bytes, cached.bytes);

    let second = compile(&second_directory).expect("compile isolated PTX artifact");
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(first.descriptor, second.descriptor);

    fs::write(&first.artifact_path, b"malformed ptx").expect("corrupt cached PTX");
    assert!(matches!(
        compile(&first_directory),
        Err(AcceleratorAotError::InvalidArtifact(_))
    ));
    fs::remove_dir_all(first_directory).expect("remove first AOT fixture");
    fs::remove_dir_all(second_directory).expect("remove second AOT fixture");
}

#[test]
fn llvm_nvptx_rejects_unadmitted_toolchain_and_target_before_execution() {
    let ir = accelerator_ir();
    let directory = output_directory("rejected");
    let toolchain = AcceleratorAdmittedToolchain {
        name: "forged".to_string(),
        version: "0".to_string(),
        executable: "/does/not/exist".to_string(),
        executable_sha256: "0".repeat(64),
        license: "unknown".to_string(),
    };
    let error = LlvmNvptxBackend
        .compile(&AcceleratorAotRequest {
            ir: &ir,
            architecture: "cpu",
            toolchain: &toolchain,
            build_options: BTreeMap::new(),
            output_directory: &directory,
        })
        .expect_err("forged toolchain must fail");
    assert!(matches!(error, AcceleratorAotError::Toolchain(_)));
    assert!(!directory.exists());
}

#[test]
fn llvm_nvptx_preserves_device_buffer_access_and_alignment() {
    let executable = Path::new("/usr/bin/llc");
    if !executable.is_file() {
        return;
    }
    let ir = buffer_ir();
    ir.verify().expect("verify device-buffer fixture");
    let toolchain = llvm_toolchain(executable);
    let directory = output_directory("buffer");
    let artifact = LlvmNvptxBackend
        .compile(&AcceleratorAotRequest {
            ir: &ir,
            architecture: "sm-30",
            toolchain: &toolchain,
            build_options: BTreeMap::from([("optimization".to_string(), "2".to_string())]),
            output_directory: &directory,
        })
        .expect("compile buffer PTX");
    let ptx = String::from_utf8(artifact.bytes).expect("UTF-8 PTX");
    assert!(ptx.contains(".visible .entry copy_first("));
    assert!(ptx.contains("ld.global.f32"));
    assert!(ptx.contains("st.global.f32"));
    fs::remove_dir_all(directory).expect("remove buffer AOT fixture");
}
