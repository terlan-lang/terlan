use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use crate::terlan_quality::terlan_polars_package::REQUIRED_TESTS;

const CONSUMER_PROJECTS: &[&str] = &[
    "consumer_project",
    "loaded_helper_project",
    "iris_dataset_audit",
    "polars_getting_started",
    "polars_expressions",
    "polars_series",
    "polars_io_formats",
    "polars_lazy_full",
    "polars_relational",
    "polars_reshape",
    "polars_advanced_relational",
    "polars_expression_namespaces",
];

/// Runs every package consumer and executable Terlan test.
pub(super) fn run_terlan_consumer_projects(package_root: &Path) -> Result<(), String> {
    let terlc = resolve_terlc_executable()?;
    run_parallel(CONSUMER_PROJECTS, |project| {
        run_consumer(package_root, &terlc, project)
    })?;
    validate_unsupported_native_target(package_root, &terlc)?;
    run_parallel(REQUIRED_TESTS, |test| {
        run_package_test(package_root, &terlc, test)
    })
}

fn run_parallel(
    names: &[&str],
    execute: impl Fn(&str) -> Result<(), String> + Sync,
) -> Result<(), String> {
    thread::scope(|scope| {
        let handles = names
            .iter()
            .map(|&name| {
                let execute = &execute;
                (name, scope.spawn(move || execute(name)))
            })
            .collect::<Vec<_>>();
        let mut failures = Vec::new();
        for (name, handle) in handles {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => failures.push(error),
                Err(_) => failures.push(format!(
                    "error[terlan_polars_execution_panic]: `{name}` execution thread panicked"
                )),
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    })
}

fn run_consumer(package_root: &Path, terlc: &Path, project: &str) -> Result<(), String> {
    let project_path = package_root.join("examples").join(project);
    let out_dir = package_root.join("target/terlan-quality").join(project);
    let output = Command::new(terlc)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("run")
        .arg(&project_path)
        .current_dir(package_root)
        .output()
        .map_err(|error| {
            format!(
                "{}: failed to run Terlan consumer with `{}`: {error}",
                project_path.display(),
                terlc.display()
            )
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}: Terlan consumer failed with status {}\nstdout:\n{}\nstderr:\n{}",
            project_path.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn run_package_test(package_root: &Path, terlc: &Path, test: &str) -> Result<(), String> {
    let test_path = package_root.join("test").join(test);
    let test_name = test.strip_suffix(".terl").unwrap_or(test);
    let cache_dir = package_root
        .join("target/terlan-quality/package-tests")
        .join(test_name)
        .join(".terlan");
    let helper = package_root.join("native/target/debug/terlan-polars-native-boundary");
    let output = Command::new(terlc)
        .arg("--cache-dir")
        .arg(&cache_dir)
        .arg("test")
        .arg(&test_path)
        .args(["--target", "terlan-vm"])
        .env("TERLAN_NATIVE_BOUNDARY_HELPER_PATH", &helper)
        .current_dir(package_root)
        .output()
        .map_err(|error| {
            format!(
                "{}: failed to execute package test with `{}`: {error}",
                test_path.display(),
                terlc.display()
            )
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}: Terlan package test execution failed with status {}\nstdout:\n{}\nstderr:\n{}",
            test_path.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn validate_unsupported_native_target(package_root: &Path, terlc: &Path) -> Result<(), String> {
    let project_path = package_root.join("examples/consumer_project");
    let out_dir = package_root.join("target/terlan-quality/unsupported-js-target");
    let output = Command::new(terlc)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("build")
        .arg(&project_path)
        .args(["--target", "js.shared"])
        .current_dir(package_root)
        .output()
        .map_err(|error| {
            format!(
                "{}: failed to probe unsupported package-native target with `{}`: {error}",
                project_path.display(),
                terlc.display()
            )
        })?;
    if output.status.success() {
        return Err(format!(
            "{}: js.shared unexpectedly accepted native Polars dependency",
            project_path.display()
        ));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    for required in [
        "error[package_native_target_unsupported]",
        "target `js.shared`",
        "local dependency `terlan-polars`",
        "capability `native-process-helper`",
        "target `terlan-vm`",
    ] {
        if !stderr.contains(required) {
            return Err(format!(
                "{}: unsupported target diagnostic is missing `{required}`\nstderr:\n{stderr}",
                project_path.display()
            ));
        }
    }
    Ok(())
}

fn resolve_terlc_executable() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("TERLC") {
        return Ok(PathBuf::from(path));
    }
    let current = env::current_exe()
        .map_err(|error| format!("failed to resolve terlan-quality executable: {error}"))?;
    let name = if cfg!(windows) { "terlc.exe" } else { "terlc" };
    let sibling = current
        .parent()
        .map(|parent| parent.join(name))
        .unwrap_or_else(|| PathBuf::from(name));
    if sibling.is_file() {
        Ok(sibling)
    } else {
        Err(format!(
            "terlc executable was not found beside terlan-quality at {} (set TERLC to override)",
            sibling.display()
        ))
    }
}
