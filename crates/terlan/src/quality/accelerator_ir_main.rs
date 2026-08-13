#![forbid(unsafe_code)]

//! Emits a deterministic checked-CoreIR to AcceleratorIR quality report.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;
use terlan::compiler::accelerator::{
    AcceleratorExecutionDimensions, AcceleratorIrInterpreter, AcceleratorIrModule,
    AcceleratorIrSource, AcceleratorIrValue, AcceleratorKernelSelection,
};
use terlan::compiler::hir::resolve_syntax_module_output;
use terlan::compiler::syntax::parse_module_as_syntax_output;
use terlan::compiler::typeck::{
    lower_syntax_module_output_to_core, type_check_syntax_module_output,
};
use terlan::support::boundary_error::QualityResult;

/// Stable AC4 evidence report.
#[derive(Serialize)]
struct AcceleratorIrReport {
    /// Stable report schema.
    schema: &'static str,
    /// Normalized backend-neutral IR.
    ir: AcceleratorIrModule,
    /// SHA-256 identity of the normalized IR.
    normalized_hash: String,
    /// Reference interpreter result for the fixture invocation.
    interpreter_result: i64,
    /// Admitted bounded language constructs.
    admitted_constructs: Vec<&'static str>,
    /// Rejected effect and execution classes covered by tests.
    rejected_constructs: Vec<&'static str>,
    /// Statement proving the report does not invoke a backend.
    backend_invoked: bool,
}

/// Fixture lowered through the public parser, resolver, typechecker, and CoreIR pipeline.
const FIXTURE: &str = "\
module accelerator_report.\n\
pub choose(left: Int, right: Int): Int ->\n\
    if { left > right -> left + 2; true -> right * 3 }.\n";

/// Parses the single report output path.
fn output_path() -> QualityResult<PathBuf> {
    let mut arguments = std::env::args().skip(1);
    let output = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: terlan-accelerator-ir <output>".to_string())?;
    if arguments.next().is_some() {
        return Err("unexpected accelerator IR report argument".into());
    }
    Ok(output)
}

/// Builds and writes the AC4 report without hardware or backend discovery.
fn run() -> QualityResult<()> {
    let output = output_path()?;
    let syntax = parse_module_as_syntax_output(FIXTURE).map_err(|error| format!("{error:?}"))?;
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    if !diagnostics.is_empty() {
        return Err(format!("accelerator report fixture diagnostics: {diagnostics:#?}").into());
    }
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let selection = AcceleratorKernelSelection {
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
            file: "target/quality/accelerator_report.terl".to_string(),
            line: 2,
            column: 1,
        },
    };
    let ir = AcceleratorIrModule::lower(&core, &[selection]).map_err(|error| error.to_string())?;
    ir.verify().map_err(|error| error.to_string())?;
    let normalized_hash = ir.normalized_hash().map_err(|error| error.to_string())?;
    let value = AcceleratorIrInterpreter::execute(
        &ir.kernels[0],
        BTreeMap::from([
            ("left".to_string(), AcceleratorIrValue::Int(5)),
            ("right".to_string(), AcceleratorIrValue::Int(3)),
        ]),
    )
    .map_err(|error| error.to_string())?;
    let AcceleratorIrValue::Int(interpreter_result) = value else {
        return Err("accelerator report fixture returned a non-integer".into());
    };
    let report = AcceleratorIrReport {
        schema: "terlan.accelerator-ir-report.v1",
        ir,
        normalized_hash,
        interpreter_result,
        admitted_constructs: vec![
            "checked-core-ir",
            "scalar-arithmetic",
            "comparison",
            "structured-branch",
            "static-loop",
            "bounds-checked-buffer-access",
            "package-declared-math",
        ],
        rejected_constructs: vec![
            "recursion",
            "dynamic-call",
            "actor-operation",
            "runtime-intrinsic",
            "exception",
            "closure",
            "unbounded-allocation",
            "undeclared-package-operation",
            "invalid-memory-contract",
        ],
        backend_invoked: false,
    };
    let parent = output
        .parent()
        .ok_or_else(|| "accelerator IR output has no parent".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let encoded = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("cannot encode accelerator IR report: {error}"))?;
    fs::write(&output, encoded + "\n")
        .map_err(|error| format!("cannot write {}: {error}", output.display()))?;
    Ok(())
}

/// Runs the deterministic AC4 report emitter.
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error[accelerator.ir-report]: {error}");
            ExitCode::from(1)
        }
    }
}
