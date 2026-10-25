//! Publishes a VM-loadable native image from typed self-hosted module sidecars.

use std::env;
use std::fs;
use std::path::PathBuf;

use terlan::self_host_ir::BackendEnvelope;

fn usage(program: &str) -> String {
    format!(
        "usage: {program} <output.tvm> <core.json> <variants.json> <actors.json> [<core.json> <variants.json> <actors.json> ...]"
    )
}

fn run() -> Result<(), String> {
    let mut arguments = env::args();
    let program = arguments
        .next()
        .unwrap_or_else(|| "terlan-self-host-image".to_string());
    let output = PathBuf::from(arguments.next().ok_or_else(|| usage(&program))?);
    let inputs = arguments.map(PathBuf::from).collect::<Vec<_>>();
    if inputs.is_empty() || inputs.len() % 3 != 0 {
        return Err(usage(&program));
    }
    let mut envelopes = Vec::new();
    let mut variant_irs = Vec::new();
    let mut actor_irs = Vec::new();
    for input in inputs.chunks_exact(3) {
        let core_source = fs::read_to_string(&input[0])
            .map_err(|error| format!("failed to read `{}`: {error}", input[0].display()))?;
        envelopes.push(
            serde_json::from_str::<BackendEnvelope>(&core_source).map_err(|error| {
                format!("invalid ABI-1 JSON in `{}`: {error}", input[0].display())
            })?,
        );
        variant_irs.push(
            fs::read_to_string(&input[1])
                .map_err(|error| format!("failed to read `{}`: {error}", input[1].display()))?,
        );
        actor_irs.push(
            fs::read_to_string(&input[2])
                .map_err(|error| format!("failed to read `{}`: {error}", input[2].display()))?,
        );
    }
    terlan::self_host_native_backend::publish_typed_actor_application_image(
        &envelopes,
        &variant_irs,
        &actor_irs,
        &output,
    )
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
