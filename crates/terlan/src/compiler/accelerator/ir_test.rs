//! Tests for checked CoreIR to AcceleratorIR lowering.

use super::*;

use crate::{
    terlan_hir::resolve_syntax_module_output,
    terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::{
        lower_syntax_module_output_to_core, type_check_syntax_module_output, CoreModule,
    },
};

/// Produces checked CoreIR from one complete Terlan module.
fn checked_core(source: &str) -> CoreModule {
    let syntax = parse_module_as_syntax_output(source).expect("parse AcceleratorIR fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

/// Returns the canonical test selection for one scalar kernel.
fn selection(function: &str) -> AcceleratorKernelSelection {
    AcceleratorKernelSelection {
        function: function.to_string(),
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
            file: "accelerator_fixture.terl".to_string(),
            line: 2,
            column: 1,
        },
    }
}

#[test]
fn execution_dimensions_reject_zero_axes() {
    let error = AcceleratorExecutionDimensions {
        grid: [1, 0, 1],
        block: [1, 1, 1],
    }
    .validate()
    .expect_err("zero launch dimensions must fail");

    assert_eq!(error, AcceleratorIrError::InvalidExecutionDimensions);
}

#[test]
fn checked_core_arithmetic_and_branch_lower_and_execute() {
    let core = checked_core(
        "module accelerator_fixture.\n\
         pub choose(left: Int, right: Int): Int ->\n\
             if { left > right -> left + 2; true -> right * 3 }.\n",
    );
    let module = AcceleratorIrModule::lower(&core, &[selection("choose")])
        .expect("lower checked arithmetic kernel");
    module.verify().expect("verify arithmetic kernel");
    let kernel = &module.kernels[0];

    let result = AcceleratorIrInterpreter::execute(
        kernel,
        BTreeMap::from([
            ("left".to_string(), AcceleratorIrValue::Int(5)),
            ("right".to_string(), AcceleratorIrValue::Int(3)),
        ]),
    )
    .expect("execute true branch");
    assert_eq!(result, AcceleratorIrValue::Int(7));

    let result = AcceleratorIrInterpreter::execute(
        kernel,
        BTreeMap::from([
            ("left".to_string(), AcceleratorIrValue::Int(2)),
            ("right".to_string(), AcceleratorIrValue::Int(4)),
        ]),
    )
    .expect("execute fallback branch");
    assert_eq!(result, AcceleratorIrValue::Int(12));
}

#[test]
fn deterministic_ir_identity_preserves_source_and_specialization() {
    let core = checked_core(
        "module deterministic_kernel.\n\
         pub add(left: Float, right: Float): Float -> left + right.\n",
    );
    let mut kernel = selection("add");
    kernel.specializations.insert(
        "T".to_string(),
        AcceleratorIrType::Scalar {
            dtype: AcceleratorScalarType::F64,
        },
    );
    let first = AcceleratorIrModule::lower(&core, &[kernel.clone()]).expect("first lowering");
    let second = AcceleratorIrModule::lower(&core, &[kernel]).expect("second lowering");

    assert_eq!(
        first.normalized_hash().unwrap(),
        second.normalized_hash().unwrap()
    );
    assert_eq!(first.kernels[0].source.file, "accelerator_fixture.terl");
    assert_eq!(first.kernels[0].specializations.len(), 1);
    assert_eq!(
        AcceleratorIrInterpreter::execute(
            &first.kernels[0],
            BTreeMap::from([
                ("left".to_string(), AcceleratorIrValue::Float(1.25)),
                ("right".to_string(), AcceleratorIrValue::Float(2.5)),
            ]),
        )
        .unwrap(),
        AcceleratorIrValue::Float(3.75)
    );
}

#[test]
fn recursion_and_unbounded_allocation_are_rejected_before_backend_work() {
    let recursion = checked_core(
        "module recursive_kernel.\n\
         pub recurse(value: Int): Int -> recurse(value).\n",
    );
    let error = AcceleratorIrModule::lower(&recursion, &[selection("recurse")])
        .expect_err("recursive call must fail");
    assert!(matches!(error, AcceleratorIrError::DynamicCall(_)));

    let allocation = checked_core(
        "module allocating_kernel.\n\
         pub allocate(value: Int): List[Int] -> [value].\n",
    );
    let error = AcceleratorIrModule::lower(&allocation, &[selection("allocate")])
        .expect_err("list allocation must fail");
    assert!(matches!(error, AcceleratorIrError::UnsupportedType(_)));
}

#[test]
fn verifier_and_interpreter_enforce_static_bounds_and_memory_contracts() {
    let source = AcceleratorIrSource {
        file: "manual_kernel.terl".to_string(),
        line: 1,
        column: 1,
    };
    let int = AcceleratorIrType::Scalar {
        dtype: AcceleratorScalarType::I64,
    };
    let local = |name: &str| AcceleratorIrNode {
        ty: int.clone(),
        source: source.clone(),
        operation: AcceleratorIrOperation::Local {
            name: name.to_string(),
        },
    };
    let body = AcceleratorIrNode {
        ty: int.clone(),
        source: source.clone(),
        operation: AcceleratorIrOperation::StaticLoop {
            index_name: "index".to_string(),
            start: 0,
            end: 4,
            accumulator_name: "sum".to_string(),
            initial: Box::new(AcceleratorIrNode {
                ty: int.clone(),
                source: source.clone(),
                operation: AcceleratorIrOperation::Int { value: 0 },
            }),
            body: Box::new(AcceleratorIrNode {
                ty: int.clone(),
                source: source.clone(),
                operation: AcceleratorIrOperation::Binary {
                    operation: AcceleratorIrBinaryOperation::Add,
                    left: Box::new(local("sum")),
                    right: Box::new(local("index")),
                },
            }),
        },
    };
    let module = AcceleratorIrModule {
        schema: ACCELERATOR_IR_SCHEMA,
        module: "manual".to_string(),
        kernels: vec![AcceleratorIrKernel {
            name: "sum".to_string(),
            core_identity: "manual.sum/0".to_string(),
            specializations: BTreeMap::new(),
            parameters: Vec::new(),
            return_type: int,
            dimensions: AcceleratorExecutionDimensions {
                grid: [1, 1, 1],
                block: [1, 1, 1],
            },
            shared_memory_bytes: 0,
            synchronization_points: Vec::new(),
            body,
            source,
        }],
    };
    module.verify().expect("bounded loop verifies");
    assert_eq!(
        AcceleratorIrInterpreter::execute(&module.kernels[0], BTreeMap::new()).unwrap(),
        AcceleratorIrValue::Int(6)
    );

    let mut invalid = module.clone();
    if let AcceleratorIrOperation::StaticLoop { end, .. } = &mut invalid.kernels[0].body.operation {
        *end = 1_000_001;
    }
    assert_eq!(
        invalid.verify().unwrap_err(),
        AcceleratorIrError::InvalidStaticLoop
    );

    let mut invalid = module;
    invalid.kernels[0].shared_memory_bytes = ACCELERATOR_IR_MAX_SHARED_MEMORY_BYTES + 1;
    assert!(matches!(
        invalid.verify().unwrap_err(),
        AcceleratorIrError::InvalidMemoryContract(_)
    ));
}

#[test]
fn interpreter_rejects_overflow_bad_arguments_and_out_of_bounds_loads() {
    let core = checked_core(
        "module overflow_kernel.\n\
         pub add(left: Int, right: Int): Int -> left + right.\n",
    );
    let module = AcceleratorIrModule::lower(&core, &[selection("add")]).unwrap();
    assert!(matches!(
        AcceleratorIrInterpreter::execute(
            &module.kernels[0],
            BTreeMap::from([
                ("left".to_string(), AcceleratorIrValue::Int(i64::MAX)),
                ("right".to_string(), AcceleratorIrValue::Int(1)),
            ]),
        ),
        Err(AcceleratorIrError::TypeMismatch(_))
    ));
    assert!(matches!(
        AcceleratorIrInterpreter::execute(&module.kernels[0], BTreeMap::new()),
        Err(AcceleratorIrError::UnknownLocal(_))
    ));
}
