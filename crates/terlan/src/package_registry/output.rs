//! Stable filesystem output and CLI handling for Registry protocol bundles.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use sha2::{Digest as _, Sha256};

use super::fixtures::fixture_documents;
use super::model::PROTOCOL_VERSION;
use super::schema::schema_documents;

/// Summary of one generated Registry protocol bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolBundleSummary {
    pub protocol: &'static str,
    pub output_directory: PathBuf,
    pub schema_count: usize,
    pub fixture_count: usize,
}

#[derive(Serialize)]
struct BundleManifest {
    schema: &'static str,
    protocol: &'static str,
    schemas: usize,
    fixtures: usize,
    files: Vec<BundleFile>,
}

#[derive(Serialize)]
struct BundleFile {
    path: String,
    sha256: String,
    bytes: usize,
}

/// Writes the versioned Registry schemas, fixtures, and integrity manifest.
pub(crate) fn write_protocol_bundle(
    output_directory: &Path,
) -> Result<ProtocolBundleSummary, String> {
    let schema_directory = output_directory.join("schemas");
    let fixture_directory = output_directory.join("fixtures");
    fs::create_dir_all(&schema_directory)
        .map_err(|error| format!("failed to create {}: {error}", schema_directory.display()))?;
    fs::create_dir_all(&fixture_directory)
        .map_err(|error| format!("failed to create {}: {error}", fixture_directory.display()))?;

    let schemas = schema_documents();
    let fixtures = fixture_documents().map_err(|error| error.to_string())?;
    let mut files = Vec::with_capacity(schemas.len() + fixtures.len());
    for document in &schemas {
        write_document(
            output_directory,
            &format!("schemas/{}", document.file_name),
            &document.value,
            &mut files,
        )?;
    }
    for document in &fixtures {
        write_document(
            output_directory,
            &format!("fixtures/{}", document.file_name),
            &document.value,
            &mut files,
        )?;
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let manifest = BundleManifest {
        schema: "terlan-registry-protocol-bundle-v1",
        protocol: PROTOCOL_VERSION,
        schemas: schemas.len(),
        fixtures: fixtures.len(),
        files,
    };
    let bytes = json_bytes(&manifest)?;
    fs::write(output_directory.join("manifest.json"), bytes).map_err(|error| {
        format!(
            "failed to write {}: {error}",
            output_directory.join("manifest.json").display()
        )
    })?;

    Ok(ProtocolBundleSummary {
        protocol: PROTOCOL_VERSION,
        output_directory: output_directory.to_path_buf(),
        schema_count: schemas.len(),
        fixture_count: fixtures.len(),
    })
}

pub(crate) fn run_protocol_command(args: &[String], output_directory: &Path) -> ExitCode {
    if args != ["protocol"] {
        eprintln!("{}", protocol_usage());
        return ExitCode::from(2);
    }
    match write_protocol_bundle(output_directory) {
        Ok(summary) => {
            println!(
                "wrote {} Registry protocol schemas and {} fixtures to {}",
                summary.schema_count,
                summary.fixture_count,
                summary.output_directory.display()
            );
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

fn protocol_usage() -> String {
    "usage: terlc package protocol --out-dir <dir>".into()
}

fn write_document<T: Serialize>(
    root: &Path,
    relative_path: &str,
    value: &T,
    files: &mut Vec<BundleFile>,
) -> Result<(), String> {
    let bytes = json_bytes(value)?;
    let path = root.join(relative_path);
    fs::write(&path, &bytes)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    files.push(BundleFile {
        path: relative_path.into(),
        sha256: Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        bytes: bytes.len(),
    });
    Ok(())
}

fn json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}
