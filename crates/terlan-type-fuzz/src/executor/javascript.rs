use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::oracle::{self, OracleResult};
use crate::Oracle;

const WORKSPACE: &str = ".type-fuzz-js";

#[derive(Clone, Copy)]
struct Limits {
    wall_ms: u64,
    rss_bytes: u64,
    cpu_ms: u64,
}

pub(super) fn run(
    compiler: &Path,
    root: &Path,
    sources: &[PathBuf],
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
) -> OracleResult {
    let workspace = root.join(WORKSPACE);
    let limits = Limits {
        wall_ms: timeout_ms,
        rss_bytes: rss_limit_bytes,
        cpu_ms: cpu_limit_ms,
    };
    let result = run_in_workspace(
        compiler,
        Path::new("node"),
        root,
        sources,
        &workspace,
        limits,
    );
    finish(result, &workspace, limits)
}

fn finish(result: OracleResult, workspace: &Path, limits: Limits) -> OracleResult {
    match remove_workspace(workspace) {
        Ok(()) => result,
        Err(error) if result.outcome == "passed" => infrastructure(
            format!("clean JavaScript oracle workspace: {error}"),
            limits,
        ),
        Err(_) => result,
    }
}

fn run_in_workspace(
    compiler: &Path,
    node: &Path,
    root: &Path,
    sources: &[PathBuf],
    workspace: &Path,
    limits: Limits,
) -> OracleResult {
    let modules = match prepare_workspace(root, sources, workspace) {
        Ok(modules) => modules,
        Err(error) => return infrastructure(error, limits),
    };
    let stage = workspace.join("src");
    let output = workspace.join("out");
    let mut build = oracle::base_command(compiler);
    build
        .arg("--out-dir")
        .arg(&output)
        .arg("build")
        .arg(&stage)
        .arg("--target")
        .arg("js.shared")
        .current_dir(root);
    let build = oracle::capture_result(
        build,
        Oracle::JavaScript,
        root,
        limits.wall_ms,
        limits.rss_bytes,
        limits.cpu_ms,
    );
    if build.outcome != "passed" {
        return build;
    }
    let js_root = output.join("js");
    if let Err(error) = write_harness(&js_root, &modules) {
        return infrastructure(error, limits);
    }
    oracle::finish_phase(
        build,
        limits.wall_ms,
        limits.rss_bytes,
        limits.cpu_ms,
        |remaining_wall, remaining_cpu| {
            let mut command = Command::new(node);
            command.arg("type-fuzz-harness.mjs").current_dir(&js_root);
            oracle::capture_result(
                command,
                Oracle::JavaScript,
                root,
                remaining_wall,
                limits.rss_bytes,
                remaining_cpu,
            )
        },
    )
}

fn prepare_workspace(
    root: &Path,
    sources: &[PathBuf],
    workspace: &Path,
) -> Result<Vec<PathBuf>, String> {
    remove_workspace(workspace).map_err(|error| format!("reset JavaScript oracle: {error}"))?;
    let stage = create_stage(workspace)?;
    let mut modules = Vec::with_capacity(sources.len());
    let mut seen = BTreeSet::new();
    for source in sources {
        let source_path = root.join(source);
        let text = fs::read_to_string(&source_path)
            .map_err(|error| format!("read generated JavaScript case: {error}"))?;
        let module = module_relative_path(&text)?;
        if !seen.insert(module.clone()) {
            return Err(format!(
                "duplicate generated JavaScript module `{}`",
                module.display()
            ));
        }
        stage_source(&stage, &module, &text)?;
        modules.push(module.with_extension("js"));
    }
    Ok(modules)
}

fn create_stage(workspace: &Path) -> Result<PathBuf, String> {
    let stage = workspace.join("src");
    fs::create_dir_all(&stage).map_err(|error| format!("create JavaScript stage: {error}"))?;
    Ok(stage)
}

fn stage_source(stage: &Path, module: &Path, text: &str) -> Result<(), String> {
    let destination = stage.join(module).with_extension("terl");
    let parent = destination
        .parent()
        .expect("a staged module always belongs to its stage root");
    fs::create_dir_all(parent)
        .map_err(|error| format!("create JavaScript module stage: {error}"))?;
    fs::write(&destination, text).map_err(|error| format!("write JavaScript module stage: {error}"))
}

fn module_relative_path(source: &str) -> Result<PathBuf, String> {
    let declaration = source
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("module "))
        .and_then(|line| line.strip_suffix('.'))
        .ok_or_else(|| {
            "generated JavaScript case has no canonical module declaration".to_string()
        })?;
    let mut path = PathBuf::new();
    for segment in declaration.split('.') {
        let mut characters = segment.chars();
        let valid = characters
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
            && characters.all(|character| character.is_ascii_alphanumeric() || character == '_');
        if !valid {
            return Err(format!(
                "generated JavaScript module has invalid segment `{segment}`"
            ));
        }
        path.push(segment);
    }
    Ok(path)
}

fn write_harness(js_root: &Path, modules: &[PathBuf]) -> Result<(), String> {
    fs::create_dir_all(js_root)
        .map_err(|error| format!("create JavaScript runtime root: {error}"))?;
    fs::write(js_root.join("package.json"), "{\"type\":\"module\"}\n")
        .map_err(|error| format!("write JavaScript runtime package: {error}"))?;
    let mut harness = String::new();
    for (index, module) in modules.iter().enumerate() {
        let module = module
            .to_str()
            .ok_or_else(|| "generated JavaScript module path is not UTF-8".to_string())?
            .replace(std::path::MAIN_SEPARATOR, "/");
        harness.push_str(&format!(
            "const case{index} = await import(\"./modules/{module}\");\nif (case{index}.sample_matches_expected() !== true) throw new Error(\"type fuzz case {index} disagreed\");\n"
        ));
    }
    fs::write(js_root.join("type-fuzz-harness.mjs"), harness)
        .map_err(|error| format!("write JavaScript runtime harness: {error}"))
}

fn remove_workspace(workspace: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(workspace) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(workspace),
        Ok(_) => fs::remove_file(workspace),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn infrastructure(message: impl Into<String>, limits: Limits) -> OracleResult {
    oracle::infrastructure_result(
        Oracle::JavaScript,
        message,
        limits.wall_ms,
        limits.rss_bytes,
        limits.cpu_ms,
    )
}

#[cfg(test)]
#[path = "javascript_test.rs"]
mod tests;
