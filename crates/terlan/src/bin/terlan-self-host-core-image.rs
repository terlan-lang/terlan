//! Publishes a VM-loadable native image from ordinary self-hosted Core envelopes.

use std::env;
use std::fs;
use std::path::PathBuf;

use terlan::self_host_ir::BackendEnvelope;

fn usage(program: &str) -> String {
    format!("usage: {program} <output.tvm> <core.json> [<core.json> ...]")
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args();
    let program = arguments
        .next()
        .unwrap_or_else(|| "terlan-self-host-core-image".to_string());
    let output = PathBuf::from(arguments.next().ok_or_else(|| usage(&program))?);
    let inputs = arguments.map(PathBuf::from).collect::<Vec<_>>();
    if inputs.is_empty() {
        return Err(usage(&program).into());
    }
    let envelopes = inputs
        .iter()
        .map(|input| {
            let source = fs::read_to_string(input)
                .map_err(|error| format!("failed to read `{}`: {error}", input.display()))?;
            serde_json::from_str::<BackendEnvelope>(&source)
                .map_err(|error| format!("invalid ABI-1 JSON in `{}`: {error}", input.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create `{}`: {error}", parent.display()))?;
    }
    terlan::self_host_native_backend::publish_application_image(&envelopes, &output)
        .map_err(Into::into)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
