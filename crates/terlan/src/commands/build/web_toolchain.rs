//! Compiler-owned Rspack, Rsbuild, and Angular.ts toolchain discovery.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

pub(crate) const ANGULAR_TS_PACKAGE: &str = "@angular-wave/angular.ts";
pub(crate) const ANGULAR_TS_VERSION: &str = "0.32.0";
pub(crate) const RSBUILD_PACKAGE: &str = "@rsbuild/core";
pub(crate) const RSBUILD_VERSION: &str = "2.1.13";
pub(crate) const RSPACK_PACKAGE: &str = "@rspack/core";
pub(crate) const RSPACK_VERSION: &str = "2.1.10";

/// Resolved immutable browser toolchain selected by the compiler.
#[derive(Debug)]
pub(crate) struct ManagedWebToolchain {
    pub(crate) root: PathBuf,
    pub(crate) rsbuild: PathBuf,
    pub(crate) angular_types: PathBuf,
}

/// Resolves and verifies the compiler-owned browser toolchain.
pub(crate) fn resolve_managed_web_toolchain() -> Result<ManagedWebToolchain, String> {
    let mut candidates = Vec::new();
    if let Some(root) = std::env::var_os("TERLAN_WEB_TOOLCHAIN_ROOT") {
        candidates.push(PathBuf::from(root));
    }
    if let Ok(executable) = std::env::current_exe() {
        for ancestor in executable.ancestors() {
            candidates.push(ancestor.join("tools/web-toolchain"));
        }
        if let Some(bin_dir) = executable.parent() {
            candidates.push(bin_dir.join("../lib/terlan/web-toolchain"));
        }
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tools/web-toolchain"),
    );

    let mut seen = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for candidate in candidates {
        let normalized = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.clone());
        if !seen.insert(normalized.clone()) {
            continue;
        }
        if !normalized.join("package.json").is_file() {
            continue;
        }
        match validate_managed_web_toolchain(&normalized) {
            Ok(toolchain) => return Ok(toolchain),
            Err(message) => diagnostics.push(message),
        }
    }
    let detail = if diagnostics.is_empty() {
        String::new()
    } else {
        format!("; {}", diagnostics.join("; "))
    };
    Err(format!(
        "error[web_toolchain_missing]: Terlan's managed web toolchain is unavailable{detail}; provision the compiler-owned lockfile with `npm ci --prefix tools/web-toolchain`"
    ))
}

fn validate_managed_web_toolchain(root: &Path) -> Result<ManagedWebToolchain, String> {
    validate_package_version(root, ANGULAR_TS_PACKAGE, ANGULAR_TS_VERSION)?;
    validate_package_version(root, RSBUILD_PACKAGE, RSBUILD_VERSION)?;
    validate_package_version(root, RSPACK_PACKAGE, RSPACK_VERSION)?;

    let rsbuild = root.join("node_modules/.bin/rsbuild");
    let angular_runtime = root
        .join("node_modules")
        .join(ANGULAR_TS_PACKAGE)
        .join("dist/angular-ts.esm.js");
    let angular_types = root
        .join("node_modules")
        .join(ANGULAR_TS_PACKAGE)
        .join("@types/namespace.d.ts");
    for required in [&rsbuild, &angular_runtime, &angular_types] {
        if !required.is_file() {
            return Err(format!(
                "managed web toolchain file is missing: {}",
                required.display()
            ));
        }
    }
    Ok(ManagedWebToolchain {
        root: root.to_path_buf(),
        rsbuild: rsbuild.canonicalize().map_err(|error| {
            format!(
                "cannot resolve managed Rsbuild binary {}: {error}",
                rsbuild.display()
            )
        })?,
        angular_types: angular_types.canonicalize().map_err(|error| {
            format!(
                "cannot resolve managed Angular.ts declarations {}: {error}",
                angular_types.display()
            )
        })?,
    })
}

fn validate_package_version(root: &Path, package: &str, expected: &str) -> Result<(), String> {
    let package_json = root.join("node_modules").join(package).join("package.json");
    let text = fs::read_to_string(&package_json).map_err(|error| {
        format!(
            "managed web package `{package}` is unavailable at {}: {error}",
            package_json.display()
        )
    })?;
    let json: Value = serde_json::from_str(&text).map_err(|error| {
        format!(
            "managed web package `{package}` has invalid metadata at {}: {error}",
            package_json.display()
        )
    })?;
    let actual = json.get("version").and_then(Value::as_str).unwrap_or("");
    if actual != expected {
        return Err(format!(
            "error[web_toolchain_drift]: managed package `{package}` must be `{expected}`, found `{actual}`"
        ));
    }
    Ok(())
}

/// Returns whether one manifest dependency is compiler-supported by the
/// managed browser toolchain.
pub(crate) fn is_managed_js_dependency(package: &str, version: &str) -> bool {
    package == ANGULAR_TS_PACKAGE && version == ANGULAR_TS_VERSION
}

/// Bundles one compiler-generated Angular.ts browser entrypoint.
///
/// Inputs:
/// - `entry`: generated JavaScript entrypoint that imports Angular.ts.
/// - `output_dir`: existing destination directory for the single bundle.
/// - `output_file`: stable JavaScript file name within `output_dir`.
///
/// Output:
/// - Success after the compiler-owned Rsbuild toolchain emits the bundle.
///
/// Transformation:
/// - Generates private Rsbuild configuration beside the entrypoint, aliases
///   Angular.ts to the pinned managed runtime, disables HTML generation, and
///   forces one stable browser chunk. Project code never owns bundler config.
pub(crate) fn bundle_managed_angular_entry(
    entry: &Path,
    output_dir: &Path,
    output_file: &str,
) -> Result<(), String> {
    if output_file.is_empty()
        || Path::new(output_file)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(output_file)
        || !output_file.ends_with(".js")
    {
        return Err(format!(
            "error[web_bundle_output]: invalid managed JavaScript output file `{output_file}`"
        ));
    }
    let toolchain = resolve_managed_web_toolchain()?;
    let entry = fs::canonicalize(entry).map_err(|error| {
        format!(
            "error[web_bundle_entry]: cannot resolve managed browser entry {}: {error}",
            entry.display()
        )
    })?;
    fs::create_dir_all(output_dir).map_err(|error| {
        format!(
            "error[web_bundle_output]: cannot create managed browser output {}: {error}",
            output_dir.display()
        )
    })?;
    let output_dir = fs::canonicalize(output_dir).map_err(|error| {
        format!(
            "error[web_bundle_output]: cannot resolve managed browser output {}: {error}",
            output_dir.display()
        )
    })?;
    let build_root = entry.parent().ok_or_else(|| {
        format!(
            "error[web_bundle_entry]: cannot determine build root for {}",
            entry.display()
        )
    })?;
    let build_root_json = serde_json::to_string(build_root)
        .map_err(|error| format!("cannot serialize managed browser build root: {error}"))?;
    let config_path = build_root.join("rsbuild.terlan.generated.mjs");
    let entry_json = serde_json::to_string(&entry)
        .map_err(|error| format!("cannot serialize managed browser entry: {error}"))?;
    let output_json = serde_json::to_string(&output_dir)
        .map_err(|error| format!("cannot serialize managed browser output: {error}"))?;
    let output_file_json = serde_json::to_string(output_file)
        .map_err(|error| format!("cannot serialize managed browser file name: {error}"))?;
    let config = format!(
        "export default {{\n  root: {build_root_json},\n  source: {{ entry: {{ docs: {entry_json} }} }},\n  tools: {{ htmlPlugin: false }},\n  resolve: {{ alias: {{ '@angular-wave/angular.ts$': `${{process.env.TERLAN_WEB_TOOLCHAIN_ROOT}}/node_modules/@angular-wave/angular.ts/dist/angular-ts.esm.js` }} }},\n  performance: {{ chunkSplit: {{ strategy: 'all-in-one' }} }},\n  output: {{\n    distPath: {{ root: {output_json}, js: '.', jsAsync: '.' }},\n    cleanDistPath: false,\n    assetPrefix: './',\n    sourceMap: false,\n    filename: {{ js: {output_file_json} }}\n  }}\n}};\n"
    );
    fs::write(&config_path, config).map_err(|error| {
        format!(
            "error[web_rsbuild_config]: cannot write managed Rsbuild config {}: {error}",
            config_path.display()
        )
    })?;

    let output = Command::new(&toolchain.rsbuild)
        .arg("build")
        .arg("--config")
        .arg(&config_path)
        .env("TERLAN_WEB_TOOLCHAIN_ROOT", &toolchain.root)
        .env("NODE_PATH", toolchain.root.join("node_modules"))
        .current_dir(build_root)
        .output()
        .map_err(|error| format!("error[web_rsbuild]: failed to start Rsbuild: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "error[web_rsbuild]: Rsbuild failed for {}:\n{}{}",
            entry.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let expected = output_dir.join(output_file);
    if !expected.is_file() {
        return Err(format!(
            "error[web_bundle_output]: managed browser bundle was not emitted at {}",
            expected.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "web_toolchain_test.rs"]
mod tests;
