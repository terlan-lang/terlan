//! Deterministic continuous and release fuzz profiles.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{FuzzProfile, Oracle, ProfileRequest, RunOptions};

const RESULT_SCHEMA: &str = "terlan.type-fuzz-profile-result.v1";
const INTERFACE_FAMILIES: [&str; 3] = [
    "generic_field_receiver",
    "generic_trait_package",
    "interface_cache_transaction",
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlannedRun {
    label: String,
    options: RunOptions,
}

#[derive(Debug, Deserialize, Serialize)]
struct ProfileResult {
    schema: String,
    compiler_revision: String,
    profile: String,
    rotation: u64,
    status: String,
    runs: Vec<ResultRun>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ResultRun {
    label: String,
    oracle: String,
    seed: u64,
    cases: u32,
    families: Vec<String>,
}

pub(crate) fn run(
    request: &ProfileRequest,
    compiler: &Path,
    compiler_revision: &str,
) -> Result<(), String> {
    let runs = plan(request)?;
    fs::create_dir_all(&request.out_dir)
        .map_err(|error| format!("create fuzz profile output: {error}"))?;
    for planned in &runs {
        crate::executor::run_generated(&planned.options, compiler, compiler_revision)?;
    }
    publish_result(request, compiler_revision, &runs)
}

pub(crate) fn verify(path: &Path, compiler_revision: &str) -> Result<(), String> {
    let text =
        fs::read_to_string(path).map_err(|error| format!("read fuzz profile result: {error}"))?;
    let result: ProfileResult = serde_json::from_str(&text)
        .map_err(|error| format!("invalid fuzz profile result: {error}"))?;
    if result.schema != RESULT_SCHEMA {
        return Err(format!(
            "unsupported fuzz profile result `{}`",
            result.schema
        ));
    }
    if result.status != "passed" {
        return Err(format!("fuzz profile result ended as `{}`", result.status));
    }
    if result.compiler_revision != compiler_revision {
        return Err(format!(
            "fuzz result revision `{}` does not match candidate `{compiler_revision}`",
            result.compiler_revision
        ));
    }
    Ok(())
}

fn plan(request: &ProfileRequest) -> Result<Vec<PlannedRun>, String> {
    request.validate()?;
    let all = crate::generate::family_names();
    let javascript = crate::generate::javascript_family_names();
    Ok(match request.profile {
        FuzzProfile::Smoke => vec![planned(request, "check", 1, Oracle::Check, all, 1)],
        FuzzProfile::Candidate => cross_backend(request, 1, all, javascript, 1),
        FuzzProfile::Scheduled => {
            let rotating = request.rotation.wrapping_add(1);
            let mutated = rotating ^ 0xa076_1d64_78bd_642f;
            let mut runs = cross_backend(request, rotating, all.clone(), javascript.clone(), 2);
            runs.extend(cross_backend(request, mutated, all, javascript, 2));
            runs
        }
        FuzzProfile::Impacted => impacted(request),
    })
}

fn cross_backend(
    request: &ProfileRequest,
    seed: u64,
    all: Vec<String>,
    javascript: Vec<String>,
    repetitions: u32,
) -> Vec<PlannedRun> {
    vec![
        planned(
            request,
            "check",
            seed,
            Oracle::Check,
            all.clone(),
            repetitions,
        ),
        planned(request, "native", seed, Oracle::Native, all, repetitions),
        planned(
            request,
            "javascript",
            seed,
            Oracle::JavaScript,
            javascript,
            repetitions,
        ),
    ]
}

fn impacted(request: &ProfileRequest) -> Vec<PlannedRun> {
    owned_families(&request.changed_paths)
        .into_iter()
        .map(|(oracle, families)| {
            let label = oracle_name(oracle);
            planned(request, label, 1, oracle, families, 1)
        })
        .collect()
}

fn owned_families(paths: &[String]) -> BTreeMap<Oracle, Vec<String>> {
    let all = crate::generate::family_names();
    let javascript = crate::generate::javascript_family_names();
    let mut owned = BTreeMap::<Oracle, BTreeSet<String>>::new();
    for path in paths {
        let everything = path == "Cargo.toml"
            || path == "Cargo.lock"
            || path.starts_with("crates/terlan-type-fuzz/")
            || path.starts_with("crates/terlan/src/commands/fuzz/");
        let frontend = path.starts_with("crates/terlan/src/compiler/") || path.starts_with("std/");
        if everything || frontend {
            extend(&mut owned, Oracle::Check, &all);
            extend(&mut owned, Oracle::Native, &all);
            extend(&mut owned, Oracle::JavaScript, &javascript);
        }
        if path.starts_with("crates/terlan/src/commands/emit_js/") {
            extend(&mut owned, Oracle::JavaScript, &javascript);
        }
        if path.starts_with("crates/terlan/src/runtime/") {
            extend(&mut owned, Oracle::Native, &all);
        }
        if path.starts_with("crates/terlan/src/commands/build/")
            || path.starts_with("crates/terlan/src/formal_pipeline")
        {
            let interfaces = INTERFACE_FAMILIES.map(str::to_string);
            extend(&mut owned, Oracle::Check, &interfaces);
            extend(&mut owned, Oracle::Native, &interfaces);
        }
    }
    owned
        .into_iter()
        .map(|(oracle, families)| (oracle, families.into_iter().collect()))
        .collect()
}

fn extend(owned: &mut BTreeMap<Oracle, BTreeSet<String>>, oracle: Oracle, families: &[String]) {
    owned
        .entry(oracle)
        .or_default()
        .extend(families.iter().cloned());
}

fn planned(
    request: &ProfileRequest,
    label: &str,
    seed: u64,
    oracle: Oracle,
    families: Vec<String>,
    repetitions: u32,
) -> PlannedRun {
    let cases = u32::try_from(families.len())
        .expect("family registry fits the bounded case count")
        .saturating_mul(repetitions);
    PlannedRun {
        label: format!("{label}-s{seed}"),
        options: RunOptions {
            seed,
            cases,
            oracle,
            families,
            out_dir: request.out_dir.join(format!("{label}-s{seed}")),
            case_timeout_ms: 5_000,
            total_timeout_ms: 300_000,
            rss_limit_bytes: 2 * 1024 * 1024 * 1024,
            cpu_limit_ms: 5_000,
            ..RunOptions::default()
        },
    }
}

fn publish_result(
    request: &ProfileRequest,
    compiler_revision: &str,
    runs: &[PlannedRun],
) -> Result<(), String> {
    let result = ProfileResult {
        schema: RESULT_SCHEMA.into(),
        compiler_revision: compiler_revision.into(),
        profile: request.profile.name().into(),
        rotation: request.rotation,
        status: "passed".into(),
        runs: runs
            .iter()
            .map(|run| ResultRun {
                label: run.label.clone(),
                oracle: oracle_name(run.options.oracle).into(),
                seed: run.options.seed,
                cases: run.options.cases,
                families: run.options.families.clone(),
            })
            .collect(),
    };
    publish_serialized_result(
        &request.out_dir.join("result.json"),
        serde_json::to_vec(&result),
    )
}

fn publish_serialized_result(
    path: &Path,
    result: serde_json::Result<Vec<u8>>,
) -> Result<(), String> {
    let mut bytes = attributed_serialization(result)?;
    bytes.push(b'\n');
    crate::executor::write_artifact(path, &bytes)
}

fn attributed_serialization(result: serde_json::Result<Vec<u8>>) -> Result<Vec<u8>, String> {
    result.map_err(|error| format!("serialize fuzz profile result: {error}"))
}

fn oracle_name(oracle: Oracle) -> &'static str {
    match oracle {
        Oracle::Check => "check",
        Oracle::JavaScript => "javascript",
        Oracle::Native => "native",
        Oracle::None => "none",
    }
}

#[cfg(test)]
#[path = "profile_test.rs"]
mod tests;
