use std::fs;
use std::path::{Path, PathBuf};

use super::artifacts::atomic_write;
use super::{interface_roundtrip, oracle};
use crate::{GeneratedCase, Oracle};

const ENTRY_SOURCE: &str = "CaseTest.terl";
const PROJECT_MANIFEST: &str = "terlan.toml";
const ROUNDTRIP_ARTIFACT: &str = "package-roundtrip.json";
const CONSUMER_MANIFEST: &str = concat!(
    "[package]\n",
    "name = \"type-fuzz-consumer\"\n",
    "version = \"0.0.0\"\n\n",
    "[build]\n",
    "source_roots = [\"src\"]\n",
    "artifact = \"library\"\n\n",
    "[dependencies]\n",
    "type-fuzz-provider = { path = \"../provider\" }\n",
);
const PROVIDER_MANIFEST: &str = concat!(
    "[package]\n",
    "name = \"type-fuzz-provider\"\n",
    "version = \"0.0.0\"\n\n",
    "[build]\n",
    "source_roots = [\"src\"]\n",
    "artifact = \"library\"\n",
);

/// Re-executes a multi-module case with every provider in a path dependency.
///
/// The accepted source-backed case keeps all modules in one package. This
/// isolated layout copies only source files into a sibling library package;
/// it never carries compiler summaries, caches, or native artifacts across the
/// package boundary.
pub(super) fn run(
    directory: &Path,
    case: &GeneratedCase,
    compiler: &Path,
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
) -> Result<(), String> {
    if case.support_sources.is_empty() || case.manifest.oracle == Oracle::None {
        return Ok(());
    }
    let modules = interface_roundtrip::support_modules(&case.support_sources)?;
    let workspace = tempfile::Builder::new()
        .prefix(".package-roundtrip-")
        .tempdir_in(directory)
        .map_err(|error| format!("create package round-trip workspace: {error}"))?;
    let consumer = stage(workspace.path(), directory, case)?;
    let result = oracle::run(
        compiler,
        case.manifest.oracle,
        &consumer.join(ENTRY_SOURCE),
        timeout_ms,
        rss_limit_bytes,
        cpu_limit_ms,
    );
    let evidence = serde_json::json!({
        "schema": "terlan.type-fuzz-package-roundtrip.v1",
        "comparison": "in_workspace_and_path_dependency_accepted",
        "in_workspace_outcome": "passed",
        "path_dependency_outcome": result.outcome,
        "oracle": oracle_name(case.manifest.oracle),
        "expected_type": case.manifest.expected_type,
        "modules": modules,
        "exit_code": result.exit_code,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "resources": result.resources,
    });
    let mut bytes = serde_json::to_vec(&evidence).expect("package evidence JSON serializes");
    bytes.push(b'\n');
    atomic_write(&directory.join(ROUNDTRIP_ARTIFACT), &bytes)?;
    if result.outcome != "passed" {
        return Err(format!(
            "case {} path-dependency semantic comparison ended as {}",
            case.manifest.case_id, result.outcome
        ));
    }
    Ok(())
}

fn stage(workspace: &Path, directory: &Path, case: &GeneratedCase) -> Result<PathBuf, String> {
    let consumer = workspace.join("consumer");
    let provider = workspace.join("provider");
    create_dir(&consumer, "consumer package")?;
    create_dir(&provider, "provider package")?;
    populate(&consumer, &provider, directory, case)?;
    Ok(consumer)
}

fn populate(
    consumer: &Path,
    provider: &Path,
    directory: &Path,
    case: &GeneratedCase,
) -> Result<(), String> {
    create_dir(&consumer.join("src"), "consumer source root")?;
    write(
        &consumer.join(PROJECT_MANIFEST),
        CONSUMER_MANIFEST,
        "consumer manifest",
    )?;
    write(
        &provider.join(PROJECT_MANIFEST),
        PROVIDER_MANIFEST,
        "provider manifest",
    )?;
    copy(
        &directory.join(ENTRY_SOURCE),
        &consumer.join(ENTRY_SOURCE),
        "consumer entry",
    )?;
    for source in &case.support_sources {
        let destination = provider.join(&source.path);
        let parent = destination
            .parent()
            .expect("validated support source has a parent");
        fs::create_dir_all(parent)
            .map_err(|error| format!("create provider source directory: {error}"))?;
        copy(
            &directory.join(&source.path),
            &destination,
            "provider source",
        )?;
    }
    Ok(())
}

fn create_dir(path: &Path, subject: &str) -> Result<(), String> {
    fs::create_dir(path).map_err(|error| format!("create package round-trip {subject}: {error}"))
}

fn write(path: &Path, contents: &str, subject: &str) -> Result<(), String> {
    fs::write(path, contents)
        .map_err(|error| format!("write package round-trip {subject}: {error}"))
}

fn copy(source: &Path, destination: &Path, subject: &str) -> Result<(), String> {
    fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| format!("copy package round-trip {subject}: {error}"))
}

fn oracle_name(oracle: Oracle) -> &'static str {
    match oracle {
        Oracle::Check => "check",
        Oracle::JavaScript => "js",
        Oracle::Native => "native",
        Oracle::None => "none",
    }
}

#[cfg(test)]
#[path = "package_roundtrip_test.rs"]
mod tests;
