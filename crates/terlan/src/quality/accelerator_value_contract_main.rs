#![forbid(unsafe_code)]

//! Emits the canonical accelerator value contract and generated declarations.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use terlan::compiler::accelerator::AcceleratorDescriptor;
use terlan::compiler::accelerator::AcceleratorValueContract;
use terlan::support::boundary_error::QualityResult;

/// Parsed emitter arguments.
struct Arguments {
    /// Artifact output root.
    output: PathBuf,
    /// Generated Terlan module name.
    module: String,
    /// Optional package descriptor used to filter scalar declarations.
    descriptor: Option<PathBuf>,
}

/// Parses the small internal emitter command line.
fn arguments() -> QualityResult<Arguments> {
    let mut values = std::env::args().skip(1);
    let output = values.next().map(PathBuf::from).ok_or_else(|| {
        "usage: terlan-accelerator-value-contract <output-directory> [--module name] [--descriptor path]"
            .to_string()
    })?;
    let mut module = "generated.AcceleratorValue".to_string();
    let mut descriptor = None;
    while let Some(option) = values.next() {
        let value = values
            .next()
            .ok_or_else(|| format!("missing value for `{option}`"))?;
        match option.as_str() {
            "--module" => module = value,
            "--descriptor" => descriptor = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown option `{option}`").into()),
        }
    }
    if module.split('.').any(|segment| segment.is_empty()) {
        return Err("generated module contains an empty path segment".into());
    }
    Ok(Arguments {
        output,
        module,
        descriptor,
    })
}

/// Writes deterministic JSON and Terlan declaration artifacts.
fn run() -> QualityResult<()> {
    let arguments = arguments()?;
    let output = arguments.output;
    fs::create_dir_all(&output)
        .map_err(|error| format!("cannot create {}: {error}", output.display()))?;
    let contract = AcceleratorValueContract::canonical();
    let json = serde_json::to_string_pretty(&contract)
        .map_err(|error| format!("cannot encode accelerator value contract: {error}"))?
        + "\n";
    fs::write(output.join("accelerator-value-contract.json"), json)
        .map_err(|error| format!("cannot write accelerator value contract: {error}"))?;
    let supported_dtypes = if let Some(path) = arguments.descriptor {
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        AcceleratorDescriptor::parse(&source, &path)
            .map_err(|error| error.to_string())?
            .dtypes
    } else {
        contract
            .scalar_types
            .iter()
            .map(|dtype| dtype.id.to_string())
            .collect()
    };
    let mut declaration_path = output.clone();
    for segment in arguments.module.split('.') {
        declaration_path.push(segment);
    }
    declaration_path.set_extension("terl");
    let declarations = declaration_path
        .parent()
        .ok_or_else(|| "generated declaration path has no parent".to_string())?;
    fs::create_dir_all(declarations)
        .map_err(|error| format!("cannot create {}: {error}", declarations.display()))?;
    fs::write(
        declaration_path,
        contract
            .render_terlan_declarations_for(&arguments.module, &supported_dtypes)
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("cannot write accelerator declarations: {error}"))?;
    let rust_path = output.join("generated/accelerator_value.rs");
    fs::create_dir_all(
        rust_path
            .parent()
            .ok_or_else(|| "generated Rust adapter path has no parent".to_string())?,
    )
    .map_err(|error| format!("cannot create generated Rust adapter directory: {error}"))?;
    fs::write(
        rust_path,
        contract
            .render_rust_scalar_codec(&supported_dtypes)
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("cannot write accelerator Rust adapter: {error}"))?;
    Ok(())
}

/// Runs the contract emitter with typed process failure.
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error[accelerator.value-contract]: {error}");
            ExitCode::from(1)
        }
    }
}
