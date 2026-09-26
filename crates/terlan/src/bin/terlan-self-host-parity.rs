use std::env;
use std::fs;
use std::process::ExitCode;

use terlan::self_host_ir::{
    backend_digest, compare_bootstrap_artifacts, decode_and_validate, stable_digest,
    BootstrapArtifact,
};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("terlan-self-host-parity: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    match arguments.as_slice() {
        [command, module, tokens, syntax, semantics, envelopes, output]
            if command == "artifact-set" =>
        {
            let artifact = BootstrapArtifact {
                module_name: module.clone(),
                token_digest: digest_evidence_path(tokens)?,
                syntax_digest: digest_evidence_path(syntax)?,
                semantic_digest: digest_evidence_path(semantics)?,
                backend_digest: digest_envelope_directory(envelopes)?,
            };
            let encoded = serde_json::to_vec_pretty(&artifact)
                .map_err(|error| format!("cannot encode bootstrap artifact: {error}"))?;
            Ok(fs::write(output, encoded)
                .map_err(|error| format!("cannot write {output}: {error}"))?)
        }
        [command, input, output] if command == "canonicalize" => {
            let source = fs::read_to_string(input)
                .map_err(|error| format!("cannot read {input}: {error}"))?;
            let envelope = decode_and_validate(&source)
                .map_err(|error| format!("cannot accept {input}: {error}"))?;
            let encoded = serde_json::to_vec(&envelope)
                .map_err(|error| format!("cannot encode canonical envelope: {error}"))?;
            Ok(fs::write(output, encoded)
                .map_err(|error| format!("cannot write {output}: {error}"))?)
        }
        [command, reference, candidate] if command == "compare" => {
            let reference = read_artifact(reference)?;
            let candidate = read_artifact(candidate)?;
            let parity = compare_bootstrap_artifacts(&reference, &candidate);
            println!(
                "{}",
                serde_json::to_string_pretty(&parity)
                    .map_err(|error| format!("cannot encode parity report: {error}"))?
            );
            if parity.fixed_point {
                Ok(())
            } else {
                Err(("bootstrap artifacts did not reach a fixed point".to_owned()).into())
            }
        }
        [command, module, tokens, syntax, semantics, envelope, output]
            if command == "artifact" =>
        {
            let envelope_source = fs::read_to_string(envelope)
                .map_err(|error| format!("cannot read {envelope}: {error}"))?;
            let checked = decode_and_validate(&envelope_source)
                .map_err(|error| format!("cannot accept {envelope}: {error}"))?;
            if checked.module.name != *module {
                return Err((format!(
                    "envelope module {:?} does not match requested module {module:?}",
                    checked.module.name
                )).into());
            }
            let artifact = BootstrapArtifact {
                module_name: module.clone(),
                token_digest: digest_file(tokens)?,
                syntax_digest: digest_file(syntax)?,
                semantic_digest: digest_file(semantics)?,
                backend_digest: backend_digest(&checked)
                    .map_err(|error| format!("cannot encode backend envelope: {error}"))?,
            };
            let encoded = serde_json::to_vec_pretty(&artifact)
                .map_err(|error| format!("cannot encode bootstrap artifact: {error}"))?;
            Ok(fs::write(output, encoded)
                .map_err(|error| format!("cannot write {output}: {error}"))?)
        }
        _ => Err(
            ("usage: terlan-self-host-parity canonicalize INPUT OUTPUT\n       terlan-self-host-parity compare REFERENCE CANDIDATE\n       terlan-self-host-parity artifact MODULE TOKENS SYNTAX SEMANTICS ENVELOPE OUTPUT\n       terlan-self-host-parity artifact-set MODULE TOKENS SYNTAX SEMANTICS ENVELOPE_DIR OUTPUT"
                .to_owned()).into(),
        ),
    }
}

fn read_artifact(path: &str) -> Result<BootstrapArtifact, Box<dyn std::error::Error>> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("cannot read {path}: {error}"))?;
    Ok(serde_json::from_str(&source)
        .map_err(|error| format!("cannot decode bootstrap artifact {path}: {error}"))?)
}

fn digest_file(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(fs::read(path)
        .map(|bytes| stable_digest(&bytes))
        .map_err(|error| format!("cannot read {path}: {error}"))?)
}

fn digest_evidence_path(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot inspect evidence path {path}: {error}"))?;
    if metadata.is_file() {
        return digest_file(path);
    }
    if !metadata.is_dir() {
        return Err((format!("evidence path {path} is neither a file nor a directory")).into());
    }
    let mut entries = fs::read_dir(path)
        .map_err(|error| format!("cannot read evidence directory {path}: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot enumerate evidence directory {path}: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut framed = Vec::new();
    let mut count = 0usize;
    for entry in entries {
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        if !file_type.is_file() {
            continue;
        }
        let bytes = fs::read(entry.path())
            .map_err(|error| format!("cannot read {}: {error}", entry.path().display()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        framed.extend_from_slice(format!("{}\t{}\n", name.len(), bytes.len()).as_bytes());
        framed.extend_from_slice(name.as_bytes());
        framed.push(b'\n');
        framed.extend_from_slice(&bytes);
        framed.push(b'\n');
        count += 1;
    }
    if count == 0 {
        return Err((format!("evidence directory {path} contains no files")).into());
    }
    Ok(stable_digest(&framed))
}

fn digest_envelope_directory(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut entries = fs::read_dir(path)
        .map_err(|error| format!("cannot read envelope directory {path}: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot enumerate envelope directory {path}: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut framed = Vec::new();
    let mut count = 0usize;
    for entry in entries {
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        if !file_type.is_file() {
            continue;
        }
        let bytes = fs::read(entry.path())
            .map_err(|error| format!("cannot read {}: {error}", entry.path().display()))?;
        let source = std::str::from_utf8(&bytes)
            .map_err(|error| format!("{} is not UTF-8: {error}", entry.path().display()))?;
        if entry.path().extension().and_then(|value| value.to_str()) == Some("bundle") {
            let mut lines = source.lines();
            if lines.next() != Some("schema\tterlan.self-host.abi-package/v1") {
                return Err((format!(
                    "{} has an invalid ABI package schema",
                    entry.path().display()
                ))
                .into());
            }
            let file_name = entry.file_name().to_string_lossy().into_owned();
            for (index, line) in lines.enumerate() {
                if line.is_empty() {
                    continue;
                }
                let checked = decode_and_validate(line).map_err(|error| {
                    format!(
                        "cannot accept {} line {}: {error}",
                        entry.path().display(),
                        index + 2
                    )
                })?;
                let canonical = serde_json::to_vec(&checked).map_err(|error| {
                    format!(
                        "cannot canonicalize {} line {}: {error}",
                        entry.path().display(),
                        index + 2
                    )
                })?;
                if canonical != line.as_bytes() {
                    return Err((format!(
                        "{} line {} is not canonical ABI-1 JSON",
                        entry.path().display(),
                        index + 2
                    ))
                    .into());
                }
                let name = format!("{file_name}:{}", index + 1);
                framed
                    .extend_from_slice(format!("{}\t{}\n", name.len(), canonical.len()).as_bytes());
                framed.extend_from_slice(name.as_bytes());
                framed.push(b'\n');
                framed.extend_from_slice(&canonical);
                framed.push(b'\n');
                count += 1;
            }
            continue;
        }
        if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let checked = decode_and_validate(source)
            .map_err(|error| format!("cannot accept {}: {error}", entry.path().display()))?;
        let canonical = serde_json::to_vec(&checked)
            .map_err(|error| format!("cannot canonicalize {}: {error}", entry.path().display()))?;
        if canonical != bytes {
            return Err((format!("{} is not canonical ABI-1 JSON", entry.path().display())).into());
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        framed.extend_from_slice(format!("{}\t{}\n", name.len(), bytes.len()).as_bytes());
        framed.extend_from_slice(name.as_bytes());
        framed.push(b'\n');
        framed.extend_from_slice(&bytes);
        framed.push(b'\n');
        count += 1;
    }
    if count == 0 {
        return Err((format!("envelope directory {path} contains no ABI-1 JSON files")).into());
    }
    Ok(stable_digest(&framed))
}
