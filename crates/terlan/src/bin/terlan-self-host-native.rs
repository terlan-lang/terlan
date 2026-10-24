//! Emits a relocatable native object from authoritative self-hosted typed IR.

use std::env;
use std::fs;
use std::path::PathBuf;

use terlan::self_host_ir::BackendEnvelope;

fn usage(program: &str) -> String {
    format!(
        "usage: {program} <core-ir.json> <variant-ir.json> <actor-ir.json> <output.o>"
    )
}

fn run() -> Result<(), String> {
    let mut arguments = env::args();
    let program = arguments
        .next()
        .unwrap_or_else(|| "terlan-self-host-native".to_string());
    let core_input = arguments.next().ok_or_else(|| usage(&program))?;
    let variant_input = arguments.next().ok_or_else(|| usage(&program))?;
    let actor_input = arguments.next().ok_or_else(|| usage(&program))?;
    let output = arguments.next().ok_or_else(|| usage(&program))?;
    if arguments.next().is_some() {
        return Err(usage(&program));
    }
    let core_input = PathBuf::from(core_input);
    let variant_input = PathBuf::from(variant_input);
    let actor_input = PathBuf::from(actor_input);
    let output = PathBuf::from(output);
    let core_source = fs::read_to_string(&core_input)
        .map_err(|error| format!("failed to read `{}`: {error}", core_input.display()))?;
    let variant_source = fs::read_to_string(&variant_input)
        .map_err(|error| format!("failed to read `{}`: {error}", variant_input.display()))?;
    let actor_source = fs::read_to_string(&actor_input)
        .map_err(|error| format!("failed to read `{}`: {error}", actor_input.display()))?;
    let envelope = serde_json::from_str::<BackendEnvelope>(&core_source)
        .map_err(|error| format!("invalid self-host ABI-1 JSON: {error}"))?;
    let object = terlan::self_host_native_backend::emit_typed_actor_object(
        &envelope,
        &variant_source,
        &actor_source,
    )?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create `{}`: {error}", parent.display()))?;
    }
    fs::write(&output, object)
        .map_err(|error| format!("failed to write `{}`: {error}", output.display()))?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
