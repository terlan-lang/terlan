//! Receipt-backed ownership for the root compiler bootstrap.
//!
//! The publication preparation graph owns one invocation of this command. A
//! successful receipt is reusable only when its caller-supplied input
//! fingerprint and every declared output byte match. Failed or interrupted
//! commands never publish a successful receipt.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use terlan_process_owner::ProcessControl;
use terlan_test_orchestrator::build_inputs::{CargoInputFiles, SourceInputs};

#[path = "owner_bootstrap.rs"]
mod bootstrap;
#[path = "owner_environment.rs"]
mod environment;

const SCHEMA: &str = "terlan.build-owner.v1";
pub(crate) const PROTOCOL: &str = "terlan.build-owner.v4";
const MAX_RECEIPT_BYTES: u64 = 16 * 1024 * 1024;

struct Options {
    root: PathBuf,
    receipt: PathBuf,
    input_sha256: String,
    timeout: Duration,
    outputs: Vec<PathBuf>,
    command: Option<Command>,
    completed_cargo_log: Option<PathBuf>,
    environment: environment::Snapshot,
    cargo: Option<PathBuf>,
    source_revision: Option<String>,
}

/// Runs a receipt-backed producer, returning false only for a producer failure.
pub(crate) fn run(root: &Path, args: &[OsString]) -> io::Result<bool> {
    let mut options = parse(root, args)?;
    let admission_timeout = options.timeout.min(Duration::from_secs(30));
    let mut source_inputs = options
        .source_revision
        .as_deref()
        .map(|revision| SourceInputs::capture(root, revision, admission_timeout))
        .transpose()?;
    let mut cargo_inputs = options
        .cargo
        .as_ref()
        .map(|cargo| CargoInputFiles::capture(cargo, options.timeout.min(Duration::from_secs(30))))
        .transpose()?;
    if options.command.is_some()
        && reusable(
            &options,
            cargo_inputs.as_ref().map(CargoInputFiles::digest),
            source_inputs.as_ref().map(SourceInputs::digest),
        )?
    {
        if let Some(inputs) = &mut cargo_inputs {
            inputs.verify(options.timeout.min(Duration::from_secs(30)))?;
        }
        if let Some(inputs) = &mut source_inputs {
            inputs.verify(admission_timeout)?;
        }
        println!(
            "{}",
            json!({
                "schema": SCHEMA,
                "decision": "reused",
                "receipt": options.receipt,
                "input_sha256": options.input_sha256,
            })
        );
        return Ok(true);
    }

    let cargo_observation = if let Some(command) = options.command.as_mut() {
        command.current_dir(root);
        options.environment.configure(command);
        if let Err(error) = ProcessControl::new(options.timeout).run(command, |_| Ok(())) {
            eprintln!("error[build.owner.{}]: {}", error.kind, error.detail);
            return Ok(false);
        }
        None
    } else {
        Some(bootstrap::validate(&options)?)
    };
    let outputs = match output_hashes(root, &options.outputs) {
        Ok(outputs) => outputs,
        Err(error) => {
            eprintln!("error[build.owner.outputs]: {error}");
            return Ok(false);
        }
    };
    let mut receipt = json!({
        "schema": SCHEMA,
        "outcome": "pass",
        "owner_protocol": PROTOCOL,
        "input_sha256": options.input_sha256,
        "environment_sha256": options.environment.digest(),
        "outputs": outputs,
    });
    if let Some(observation) = cargo_observation {
        receipt["bootstrap_cargo_log_sha256"] = observation.into();
    }
    if let Some(inputs) = &mut cargo_inputs {
        inputs.verify(options.timeout.min(Duration::from_secs(30)))?;
        receipt["cargo_inputs_sha256"] = inputs.digest().into();
    }
    if let Some(inputs) = &mut source_inputs {
        inputs.verify(admission_timeout)?;
        receipt["source_sha256"] = inputs.digest().into();
        receipt["source_revision"] = options.source_revision.clone().into();
    }
    owned_file_path(root, &options.receipt)?;
    publish_receipt(&options.receipt, &receipt)?;
    println!("{}", receipt);
    Ok(true)
}

fn parse(root: &Path, args: &[OsString]) -> io::Result<Options> {
    let mut index = 0;
    let mut receipt = None;
    let mut input_sha256 = None;
    let mut timeout = None;
    let mut outputs: Vec<PathBuf> = Vec::new();
    let mut completed_cargo_log = None;
    let mut cargo = None;
    let mut source_revision = None;
    let command_start = loop {
        if index >= args.len() {
            break None;
        }
        let flag = args[index].to_str().unwrap_or("");
        index += 1;
        match flag {
            "--receipt" => receipt = Some(next_value(args, &mut index, "--receipt")?),
            "--input-sha256" => {
                input_sha256 = Some(next_value(args, &mut index, "--input-sha256")?)
            }
            "--timeout-seconds" => {
                let value = next_value(args, &mut index, "--timeout-seconds")?;
                let seconds = value
                    .to_str()
                    .filter(|value| {
                        !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                    })
                    .and_then(|value| value.parse::<u64>().ok())
                    .filter(|value| (1..=86400).contains(value))
                    .ok_or_else(|| io::Error::other("invalid owner timeout"))?;
                timeout = Some(Duration::from_secs(seconds));
            }
            "--output" => outputs.push(next_value(args, &mut index, "--output")?.into()),
            "--cargo" => cargo = Some(PathBuf::from(next_value(args, &mut index, flag)?)),
            "--source-revision" => {
                source_revision = Some(
                    next_value(args, &mut index, flag)?
                        .into_string()
                        .map_err(|_| io::Error::other("invalid source revision"))?,
                );
            }
            "--completed-cargo-log" => {
                completed_cargo_log = Some(PathBuf::from(next_value(args, &mut index, flag)?));
            }
            "--" => break Some(index),
            _ => return Err(io::Error::other(usage())),
        }
    };
    let command = if let Some(start) = command_start {
        let program = args
            .get(start)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| io::Error::other(usage()))?;
        let mut command = Command::new(program);
        command.args(&args[start + 1..]);
        Some(command)
    } else {
        None
    };
    if command.is_some() == completed_cargo_log.is_some() {
        return Err(io::Error::other(usage()));
    }
    let receipt = receipt.ok_or_else(|| io::Error::other(usage()))?;
    let input_sha256 = input_sha256
        .and_then(|value| value.into_string().ok())
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| io::Error::other("invalid owner input fingerprint"))?;
    if outputs.is_empty() {
        return Err(io::Error::other("owner requires at least one output"));
    }
    let receipt = root.join(receipt);
    if !receipt.starts_with(root)
        || receipt
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(io::Error::other("owner receipt escapes repository root"));
    }
    if outputs.iter().any(|path| {
        path.is_absolute()
            || path
                .components()
                .any(|component| component == Component::ParentDir)
    }) {
        return Err(io::Error::other("owner output escapes repository root"));
    }
    owned_file_path(root, &receipt)?;
    for output in &outputs {
        owned_file_path(root, &root.join(output))?;
    }
    Ok(Options {
        root: root.to_path_buf(),
        receipt,
        input_sha256,
        timeout: timeout.ok_or_else(|| io::Error::other(usage()))?,
        outputs,
        command,
        completed_cargo_log,
        environment: environment::Snapshot::capture()?,
        cargo,
        source_revision,
    })
}

fn next_value(args: &[OsString], index: &mut usize, flag: &str) -> io::Result<OsString> {
    let value = args
        .get(*index)
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| io::Error::other(format!("{flag} requires a value")))?;
    *index += 1;
    Ok(value)
}

fn usage() -> &'static str {
    "usage: owner [--cargo <executable>] [--source-revision <commit>] --receipt <path> --input-sha256 <hex> --timeout-seconds <1..86400> --output <path>... (-- <program> [args...] | --completed-cargo-log <path>)"
}

/// Missing descendants are allowed for cold builds, but existing components
/// must not redirect a receipt or output outside the selected checkout.
fn owned_file_path(root: &Path, path: &Path) -> io::Result<()> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| io::Error::other("owner path escapes repository root"))?;
    let components: Vec<_> = relative.components().collect();
    if components.is_empty()
        || components
            .iter()
            .any(|part| !matches!(part, Component::Normal(_)))
        || !fs::symlink_metadata(root)?.is_dir()
    {
        return Err(io::Error::other("invalid owner file path"));
    }
    let mut current = root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                let valid = if index + 1 == components.len() {
                    metadata.is_file()
                } else {
                    metadata.is_dir()
                };
                if !valid {
                    return Err(io::Error::other(format!(
                        "owner path contains a redirected or nonregular component: {}",
                        current.display()
                    )));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn reusable(
    options: &Options,
    cargo_inputs: Option<&str>,
    source_inputs: Option<&str>,
) -> io::Result<bool> {
    owned_file_path(&options.root, &options.receipt)?;
    let source = match fs::read(&options.receipt) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if source.len() as u64 > MAX_RECEIPT_BYTES {
        return Ok(false);
    }
    let document: Value = match serde_json::from_slice(&source) {
        Ok(document) => document,
        Err(_) => return Ok(false),
    };
    if document.get("schema").and_then(Value::as_str) != Some(SCHEMA)
        || document.get("owner_protocol").and_then(Value::as_str) != Some(PROTOCOL)
        || document.get("outcome").and_then(Value::as_str) != Some("pass")
        || document.get("input_sha256").and_then(Value::as_str) != Some(&options.input_sha256)
        || document.get("environment_sha256").and_then(Value::as_str)
            != Some(options.environment.digest())
        || document.get("cargo_inputs_sha256").and_then(Value::as_str) != cargo_inputs
        || document.get("source_sha256").and_then(Value::as_str) != source_inputs
        || document.get("source_revision").and_then(Value::as_str)
            != options.source_revision.as_deref()
    {
        return Ok(false);
    }
    let Some(expected) = document.get("outputs").and_then(Value::as_object) else {
        return Ok(false);
    };
    let mut expected_hashes = BTreeMap::new();
    for (path, value) in expected {
        let Some(hash) = value.as_str() else {
            return Ok(false);
        };
        expected_hashes.insert(path.clone(), hash.to_string());
    }
    let actual = match output_hashes(&options.root, &options.outputs) {
        Ok(actual) => actual,
        Err(_) => return Ok(false),
    };
    Ok(expected_hashes == actual)
}

fn output_hashes(root: &Path, paths: &[PathBuf]) -> io::Result<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    for path in paths {
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            root.join(path)
        };
        owned_file_path(root, &absolute)?;
        let metadata = fs::symlink_metadata(&absolute)?;
        if !metadata.is_file() {
            return Err(io::Error::other(format!(
                "output is not a regular file: {}",
                absolute.display()
            )));
        }
        let mut file = File::open(&absolute)?;
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        let relative = absolute
            .strip_prefix(root)
            .unwrap_or(&absolute)
            .to_string_lossy()
            .into_owned();
        hashes.insert(
            relative,
            digest
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        );
    }
    Ok(hashes)
}

fn publish_receipt(path: &Path, receipt: &Value) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("owner receipt has no parent"))?;
    fs::create_dir_all(parent)?;
    let pending = path.with_extension("json.pending");
    let bytes = serde_json::to_vec_pretty(receipt)?;
    match fs::symlink_metadata(&pending) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(io::Error::other(
                    "owner receipt pending path is not a regular file",
                ));
            }
            fs::remove_file(&pending)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&pending)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(pending, path)
}

#[cfg(test)]
#[path = "owner_test.rs"]
mod tests;
