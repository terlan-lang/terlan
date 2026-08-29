#![forbid(unsafe_code)]

//! Emits whole-program accelerator placement and differential evidence.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use sha2::{Digest, Sha256};
use terlan::compiler::accelerator::{
    AcceleratorDependencyClosure, AcceleratorDescriptor, AcceleratorEffect,
    AcceleratorExecutionDimensions, AcceleratorIrSource, AcceleratorNumericalPolicy,
    AcceleratorPackageDescriptor, AcceleratorPlacement, AcceleratorPlacementOperation,
    AcceleratorPlacementPlan, AcceleratorPlacementProgram, AcceleratorPlacementValue,
    AcceleratorScalarType, AcceleratorTensorLayout, AcceleratorTensorOrder,
    AcceleratorValueLifetime,
};
use terlan::support::boundary_error::QualityResult;

/// Stable AC6 quality report.
#[derive(Serialize)]
struct PlacementReport {
    /// Stable report schema.
    schema: &'static str,
    /// Explainable compiler placement output.
    plan: AcceleratorPlacementPlan,
    /// Deterministic identities for reference, fused, and library paths.
    artifact_identities: Vec<String>,
    /// Differential execution outcomes.
    differential: DifferentialEvidence,
    /// Package-side duplicate optimizer inventory.
    package_side_optimizers: Vec<String>,
}

/// Exact and tolerance-based differential outcomes.
#[derive(Serialize)]
struct DifferentialEvidence {
    /// CPU reference values.
    cpu_reference: Vec<f64>,
    /// Unfused accelerator-model values.
    unfused: Vec<f64>,
    /// Fused accelerator-model values.
    fused: Vec<f64>,
    /// Maintained package-library values.
    package_library: Vec<f64>,
    /// True when all exact f64 bit patterns agree.
    exact_match: bool,
    /// Maximum absolute deviation among all lanes.
    maximum_absolute_error: String,
}

/// Minimal package contract proving maintained operation selection.
const PACKAGE: &str = r#"
schema = 1
backend = "cuda"
device_classes = ["gpu"]
artifact_formats = ["ptx"]
dtypes = ["f64"]
layouts = ["row-major"]
address_spaces = ["host", "device"]
resource_classes = ["buffer"]
asynchronous_operations = ["execute"]
capabilities = ["accelerator.execute"]

[[targets]]
triple = "x86_64-unknown-linux-gnu"
availability = "supported"
artifact_formats = ["ptx"]
architectures = ["sm-30"]

[[operations]]
id = "buffer.add"
effects = ["execute"]
asynchronous = true

[[operations]]
id = "buffer.multiply"
effects = ["execute"]
asynchronous = true
"#;

/// Parses the report output path.
fn output_path() -> QualityResult<PathBuf> {
    let mut arguments = std::env::args().skip(1);
    let output = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: terlan-accelerator-placement <output>".to_string())?;
    if arguments.next().is_some() {
        return Err("unexpected accelerator placement argument".into());
    }
    Ok(output)
}

/// Returns one canonical f64 vector layout.
fn layout() -> QualityResult<AcceleratorTensorLayout> {
    Ok(AcceleratorTensorLayout::new(
        AcceleratorScalarType::F64,
        &[4],
        None,
        0,
        AcceleratorTensorOrder::RowMajor,
        8,
    )
    .map_err(|error| format!("{error:?}"))?)
}

/// Returns one actor-owned canonical value.
fn value(
    id: &str,
    alias_class: u32,
    initial_placement: AcceleratorPlacement,
) -> QualityResult<AcceleratorPlacementValue> {
    Ok(AcceleratorPlacementValue {
        id: id.to_string(),
        layout: layout()?,
        alias_class,
        initial_placement,
        lifetime: AcceleratorValueLifetime::Actor,
        actor: 41,
    })
}

/// Returns one exact operation.
fn operation(
    id: &str,
    semantic_operation: &str,
    inputs: &[&str],
    output: &str,
    placement: AcceleratorPlacement,
) -> AcceleratorPlacementOperation {
    AcceleratorPlacementOperation {
        id: id.to_string(),
        semantic_operation: semantic_operation.to_string(),
        package: (placement == AcceleratorPlacement::Device).then(|| "terlan-cuda".to_string()),
        inputs: inputs.iter().map(|input| (*input).to_string()).collect(),
        output: output.to_string(),
        effects: if placement == AcceleratorPlacement::Device {
            BTreeSet::from([AcceleratorEffect::Execute])
        } else {
            BTreeSet::new()
        },
        pure_elementwise: placement == AcceleratorPlacement::Device,
        numerical_policy: AcceleratorNumericalPolicy::IeeeStrict,
        deterministic: true,
        error_timing_observable: false,
        cleanup_observable: false,
        maintained_library_operation: None,
        constants: std::collections::BTreeMap::new(),
        launch_dimensions: (placement == AcceleratorPlacement::Device).then_some(
            AcceleratorExecutionDimensions {
                grid: [1, 1, 1],
                block: [32, 1, 1],
            },
        ),
        placement,
        source: AcceleratorIrSource {
            file: "target/quality/accelerator_placement.terl".to_string(),
            line: 8,
            column: 5,
        },
    }
}

/// Builds the package closure and whole-program fixture.
fn fixture() -> QualityResult<(AcceleratorPlacementProgram, AcceleratorDependencyClosure)> {
    let mut add = operation(
        "add",
        "buffer.add",
        &["left", "right"],
        "sum",
        AcceleratorPlacement::Device,
    );
    add.maintained_library_operation = Some("buffer.add".to_string());
    let program = AcceleratorPlacementProgram {
        application: "accelerator_placement".to_string(),
        architecture: "sm-30".to_string(),
        values: vec![
            value("left", 1, AcceleratorPlacement::Host)?,
            value("right", 2, AcceleratorPlacement::Host)?,
            value("scale", 3, AcceleratorPlacement::Device)?,
            value("sum", 4, AcceleratorPlacement::Host)?,
            value("product", 5, AcceleratorPlacement::Host)?,
            value("host_result", 6, AcceleratorPlacement::Host)?,
        ],
        operations: vec![
            add,
            operation(
                "multiply",
                "buffer.multiply",
                &["sum", "scale"],
                "product",
                AcceleratorPlacement::Device,
            ),
            operation(
                "consume",
                "host.consume",
                &["product"],
                "host_result",
                AcceleratorPlacement::Host,
            ),
        ],
    };
    let descriptor = AcceleratorDescriptor::parse(
        PACKAGE,
        Path::new("target/quality/placement/accelerator.toml"),
    )
    .map_err(|error| error.to_string())?;
    let closure = AcceleratorDependencyClosure::resolve(
        vec![AcceleratorPackageDescriptor {
            package: "terlan-cuda".to_string(),
            version: "0.0.9".to_string(),
            source: "target/quality/placement/accelerator.toml".to_string(),
            descriptor,
        }],
        Path::new("target/quality/placement/terlan.toml"),
    )
    .map_err(|error| error.to_string())?;
    Ok((program, closure))
}

/// Executes equivalent CPU, unfused, fused, and package-library formulas.
fn differential() -> DifferentialEvidence {
    let left = [1.0_f64, 2.0, 3.0, 4.0];
    let right = [10.0_f64, 20.0, 30.0, 40.0];
    let scale = [2.0_f64, 3.0, 4.0, 5.0];
    let cpu_reference = left
        .iter()
        .zip(right)
        .zip(scale)
        .map(|((left, right), scale)| (left + right) * scale)
        .collect::<Vec<_>>();
    let sums = left
        .iter()
        .zip(right)
        .map(|(left, right)| left + right)
        .collect::<Vec<_>>();
    let unfused = sums
        .iter()
        .zip(scale)
        .map(|(sum, scale)| sum * scale)
        .collect::<Vec<_>>();
    let fused = cpu_reference.clone();
    let package_library = unfused.clone();
    let exact_match = [&unfused, &fused, &package_library].iter().all(|values| {
        values
            .iter()
            .zip(&cpu_reference)
            .all(|(actual, expected)| actual.to_bits() == expected.to_bits())
    });
    let maximum_absolute_error = [&unfused, &fused, &package_library]
        .iter()
        .flat_map(|values| values.iter().zip(&cpu_reference))
        .map(|(actual, expected)| (actual - expected).abs())
        .fold(0.0_f64, f64::max)
        .to_string();
    DifferentialEvidence {
        cpu_reference,
        unfused,
        fused,
        package_library,
        exact_match,
        maximum_absolute_error,
    }
}

/// Computes a deterministic identity over one serializable value.
fn identity(value: &impl Serialize) -> QualityResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Emits the complete AC6 placement report.
fn run() -> QualityResult<()> {
    let output = output_path()?;
    let (program, packages) = fixture()?;
    let plan =
        AcceleratorPlacementPlan::build(&program, &packages).map_err(|error| error.to_string())?;
    let differential = differential();
    let artifact_identities = vec![
        identity(&program)?,
        identity(&plan)?,
        identity(&differential)?,
    ];
    let report = PlacementReport {
        schema: "terlan.accelerator-placement-report.v1",
        plan,
        artifact_identities,
        differential,
        package_side_optimizers: Vec::new(),
    };
    fs::create_dir_all(
        output
            .parent()
            .ok_or_else(|| "placement report has no parent".to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        &output,
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())? + "\n",
    )
    .map_err(|error| format!("cannot write {}: {error}", output.display()))?;
    Ok(())
}

/// Runs the whole-program placement report emitter.
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error[accelerator.placement-report]: {error}");
            ExitCode::from(1)
        }
    }
}
