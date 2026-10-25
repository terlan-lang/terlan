use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const COMMAND: &str = "emit-self-hosted-actor-object";

/// Emits one actor-aware native object from paired self-hosted artifacts.
pub(crate) fn run(args: &[String]) -> ExitCode {
    let [core_ir_path, variant_ir_path, actor_ir_path, output_path] = args else {
        eprintln!("terlc {COMMAND} requires Core IR, variant IR, actor IR, and output paths");
        crate::print_command_usage(COMMAND);
        return ExitCode::from(2);
    };

    match emit(
        Path::new(core_ir_path),
        Path::new(variant_ir_path),
        Path::new(actor_ir_path),
        Path::new(output_path),
    ) {
        Ok(()) => {
            println!("[{COMMAND}] wrote {output_path}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("[{COMMAND}] failed: {error}");
            ExitCode::from(1)
        }
    }
}

fn emit(
    core_ir_path: &Path,
    variant_ir_path: &Path,
    actor_ir_path: &Path,
    output_path: &Path,
) -> Result<(), String> {
    let core_ir = fs::read_to_string(core_ir_path)
        .map_err(|error| format!("cannot read Core IR `{}`: {error}", core_ir_path.display()))?;
    let envelope = crate::self_host_ir::decode_and_validate(&core_ir).map_err(|error| {
        format!(
            "Core IR `{}` failed ABI validation: {error}",
            core_ir_path.display()
        )
    })?;
    let actor_ir = fs::read_to_string(actor_ir_path).map_err(|error| {
        format!(
            "cannot read actor IR `{}`: {error}",
            actor_ir_path.display()
        )
    })?;
    let variant_ir = fs::read_to_string(variant_ir_path).map_err(|error| {
        format!(
            "cannot read variant IR `{}`: {error}",
            variant_ir_path.display()
        )
    })?;
    let object = crate::self_host_native_backend::emit_typed_actor_object(
        &envelope,
        &variant_ir,
        &actor_ir,
    )?;
    write_atomic(output_path, &object)
}

fn write_atomic(output_path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp_path = temporary_path(output_path);
    fs::write(&temp_path, bytes).map_err(|error| {
        format!(
            "cannot write temporary object `{}`: {error}",
            temp_path.display()
        )
    })?;
    if let Err(error) = fs::rename(&temp_path, output_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!(
            "cannot publish object `{}`: {error}",
            output_path.display()
        ));
    }
    Ok(())
}

fn temporary_path(output_path: &Path) -> PathBuf {
    let mut name = output_path
        .file_name()
        .map(|value| value.to_os_string())
        .unwrap_or_else(|| "self-hosted-actor-object".into());
    name.push(".tmp");
    output_path.with_file_name(name)
}
