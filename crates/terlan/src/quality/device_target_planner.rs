use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/device-target-planner-report.json";
const PROFILE_SCHEMA_FIELDS: &[&str] = &[
    "name",
    "cpu",
    "memory_budget_bytes",
    "allocator_policy",
    "panic_strategy",
    "runtime_profile",
    "peripherals",
    "package_hal_capabilities",
    "linker_output_format",
    "rust_target",
    "unsupported_terlan_features",
    "producible_artifacts",
];
const REQUIRED_ADVERSARIAL_CASES: &[&str] = &[
    "missing device fields",
    "inconsistent memory budgets",
    "unsupported imports",
    "undeclared peripherals",
    "package/HAL mismatch",
    "nondeterministic plan ordering",
    "source-checkout path leakage",
    "plans that claim artifacts the compiler cannot produce",
];
const FUTURE_LOWERING_PREREQUISITES: &[&str] = &[
    "VM-free no_std allocator integration",
    "device HAL package availability",
    "panic strategy lowering",
    "linker script generation",
    "target-specific std subset proof",
    "artifact producer registration",
];
const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the deterministic device-target planner gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceTargetPlannerSummary {
    pub profile_count: usize,
    pub plan_hash_count: usize,
    pub rejected_feature_count: usize,
    pub diagnostic_count: usize,
    pub future_lowering_prerequisite_count: usize,
    pub report_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceProfile {
    name: String,
    cpu: String,
    memory_budget_bytes: u64,
    allocator_policy: String,
    panic_strategy: String,
    runtime_profile: String,
    peripherals: BTreeSet<String>,
    package_hal_capabilities: BTreeSet<String>,
    linker_output_format: String,
    rust_target: String,
    unsupported_terlan_features: BTreeSet<String>,
    producible_artifacts: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceTargetPlan {
    profile_name: String,
    selected_runtime: String,
    std_subset: Vec<String>,
    package_capabilities: Vec<String>,
    native_bindings: Vec<String>,
    memory_policy: String,
    rejected_imports: Vec<String>,
    required_toolchains: Vec<String>,
    output_artifacts: Vec<String>,
    diagnostics: Vec<String>,
    plan_hash: String,
}

/// Runs the deterministic device-target planner quality gate.
///
/// Inputs:
/// - `root`: repository root used for report output.
///
/// Output:
/// - A machine-readable report for built-in constrained device profiles.
/// - Stable diagnostics if any built-in profile or generated plan is invalid.
///
/// Transformation:
/// - Plans two fixture device targets from capability profiles without falling
///   back to host defaults or claiming unsupported lowering artifacts.
pub fn run_device_target_planner(root: &Path) -> QualityResult<DeviceTargetPlannerSummary> {
    let project = fixture_project();
    let profiles = built_in_profiles()?;
    let plans = profiles
        .iter()
        .map(|profile| plan_device_target(&project, profile))
        .collect::<QualityResult<Vec<_>>>()?;

    let rejected_features = profiles
        .iter()
        .flat_map(|profile| profile.unsupported_terlan_features.iter().cloned())
        .collect::<BTreeSet<_>>();
    let diagnostics = plans
        .iter()
        .flat_map(|plan| plan.diagnostics.iter().cloned())
        .collect::<BTreeSet<_>>();

    let report_path = root.join(REPORT_PATH);
    write_report(
        &report_path,
        &profiles,
        &plans,
        &rejected_features,
        &diagnostics,
    )?;

    Ok(DeviceTargetPlannerSummary {
        profile_count: profiles.len(),
        plan_hash_count: plans.len(),
        rejected_feature_count: rejected_features.len(),
        diagnostic_count: diagnostics.len(),
        future_lowering_prerequisite_count: FUTURE_LOWERING_PREREQUISITES.len(),
        report_path,
    })
}

fn built_in_profiles() -> QualityResult<Vec<DeviceProfile>> {
    [nxt_profile_json(), riscv_embedded_profile_json()]
        .iter()
        .map(|text| parse_device_profile(text))
        .collect()
}

fn nxt_profile_json() -> &'static str {
    r#"{
      "name": "nxt-arm7-constrained",
      "cpu": "ARM7TDMI",
      "memory_budget_bytes": 65536,
      "allocator_policy": "static-region",
      "panic_strategy": "abort",
      "runtime_profile": "no_std.static",
      "peripherals": ["buttons", "i2c", "lcd", "motor_pwm"],
      "package_hal_capabilities": ["hal.buttons", "hal.i2c", "hal.lcd", "hal.motor_pwm"],
      "linker_output_format": "flat-binary",
      "rust_target": "armv5te-none-eabi",
      "unsupported_terlan_features": ["actors", "database", "filesystem", "heap", "http", "native-boundary", "networking", "vm"],
      "producible_artifacts": ["device-plan.json"]
    }"#
}

fn riscv_embedded_profile_json() -> &'static str {
    r#"{
      "name": "riscv32imac-generic",
      "cpu": "rv32imac",
      "memory_budget_bytes": 262144,
      "allocator_policy": "bounded-bump",
      "panic_strategy": "abort",
      "runtime_profile": "no_std.embedded",
      "peripherals": ["gpio", "spi", "uart"],
      "package_hal_capabilities": ["hal.gpio", "hal.spi", "hal.uart"],
      "linker_output_format": "elf",
      "rust_target": "riscv32imac-unknown-none-elf",
      "unsupported_terlan_features": ["actors", "database", "filesystem", "http", "native-boundary", "networking", "vm"],
      "producible_artifacts": ["device-plan.json"]
    }"#
}

fn fixture_project() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "src/main.terl".to_string(),
            [
                "module device.Main.",
                "import std.core.Int.",
                "import std.core.Bool.",
                "import std.device.Hal.",
                "pub main(): Int -> 1.",
            ]
            .join("\n"),
        ),
        (
            "src/diagnostic_rejections.terl".to_string(),
            [
                "module device.Diagnostics.",
                "import std.http.Server.",
                "import std.io.File.",
                "import std.db.Postgres.",
                "import std.vm.Actor.",
                "import std.native.Boundary.",
            ]
            .join("\n"),
        ),
    ])
}

fn parse_device_profile(text: &str) -> QualityResult<DeviceProfile> {
    let value: Value = serde_json::from_str(text)
        .map_err(|err| format!("[device-target-planner] invalid device profile JSON: {err}"))?;
    let object = value.as_object().ok_or_else(|| {
        "[device-target-planner] device profile must be a JSON object".to_string()
    })?;
    let mut diagnostics = Vec::new();
    for field in PROFILE_SCHEMA_FIELDS {
        if !object.contains_key(*field) {
            diagnostics.push(format!("missing device profile field `{field}`"));
        }
    }
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    let memory_budget_bytes = required_u64(object, "memory_budget_bytes")?;
    if memory_budget_bytes < 4096 {
        return Err(render_failure(&[format!(
            "inconsistent memory budget `{memory_budget_bytes}`: minimum supported planning budget is 4096 bytes"
        )]));
    }

    let profile = DeviceProfile {
        name: required_string(object, "name")?,
        cpu: required_string(object, "cpu")?,
        memory_budget_bytes,
        allocator_policy: required_string(object, "allocator_policy")?,
        panic_strategy: required_string(object, "panic_strategy")?,
        runtime_profile: required_string(object, "runtime_profile")?,
        peripherals: required_string_set(object, "peripherals")?,
        package_hal_capabilities: required_string_set(object, "package_hal_capabilities")?,
        linker_output_format: required_string(object, "linker_output_format")?,
        rust_target: required_string(object, "rust_target")?,
        unsupported_terlan_features: required_string_set(object, "unsupported_terlan_features")?,
        producible_artifacts: required_string_set(object, "producible_artifacts")?,
    };
    validate_profile(&profile)?;
    Ok(profile)
}

fn validate_profile(profile: &DeviceProfile) -> QualityResult<()> {
    let mut diagnostics = Vec::new();
    for peripheral in &profile.peripherals {
        let required = format!("hal.{peripheral}");
        if !profile.package_hal_capabilities.contains(&required) {
            diagnostics.push(format!(
                "package/HAL mismatch for profile `{}`: missing capability `{required}`",
                profile.name
            ));
        }
    }
    if profile.producible_artifacts.iter().any(|artifact| {
        artifact != "device-plan.json"
            && artifact != "diagnostics.json"
            && artifact != "linker-plan.json"
    }) {
        diagnostics.push(format!(
            "profile `{}` claims artifacts the compiler cannot produce",
            profile.name
        ));
    }
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(())
}

fn plan_device_target(
    project_files: &BTreeMap<String, String>,
    profile: &DeviceProfile,
) -> QualityResult<DeviceTargetPlan> {
    let imports = collect_imports(project_files);
    let rejected_imports = imports
        .iter()
        .filter_map(|import| rejected_import(import, &profile.unsupported_terlan_features))
        .collect::<Vec<_>>();
    let diagnostics = build_plan_diagnostics(&rejected_imports, profile);
    let std_subset = imports
        .iter()
        .filter(|import| rejected_import(import, &profile.unsupported_terlan_features).is_none())
        .cloned()
        .collect::<Vec<_>>();
    let output_artifacts = profile
        .producible_artifacts
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    let required_toolchains = vec![format!("rust-target:{}", profile.rust_target)];
    let package_capabilities = profile
        .package_hal_capabilities
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    let native_bindings = profile
        .peripherals
        .iter()
        .map(|peripheral| format!("hal::{peripheral}"))
        .collect::<Vec<_>>();
    let selected_runtime = profile.runtime_profile.clone();
    let memory_policy = format!(
        "{}:{} bytes:{}",
        profile.allocator_policy, profile.panic_strategy, profile.memory_budget_bytes
    );
    let hash_input = [
        profile.name.clone(),
        profile.cpu.clone(),
        selected_runtime.clone(),
        std_subset.join("|"),
        package_capabilities.join("|"),
        rejected_imports.join("|"),
        required_toolchains.join("|"),
        output_artifacts.join("|"),
        diagnostics.join("|"),
    ]
    .join("\n");
    let plan_hash = stable_plan_hash(&hash_input);
    let plan = DeviceTargetPlan {
        profile_name: profile.name.clone(),
        selected_runtime,
        std_subset,
        package_capabilities,
        native_bindings,
        memory_policy,
        rejected_imports,
        required_toolchains,
        output_artifacts,
        diagnostics,
        plan_hash,
    };
    validate_plan_no_path_leakage(&plan)?;
    Ok(plan)
}

fn collect_imports(project_files: &BTreeMap<String, String>) -> Vec<String> {
    let mut imports = BTreeSet::new();
    for text in project_files.values() {
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("import ") {
                imports.insert(rest.trim_end_matches('.').to_string());
            }
        }
    }
    imports.into_iter().collect()
}

fn rejected_import(import: &str, unsupported_features: &BTreeSet<String>) -> Option<String> {
    [
        ("filesystem", "std.io."),
        ("networking", "std.net."),
        ("http", "std.http."),
        ("database", "std.db."),
        ("actors", "std.vm.Actor"),
        ("vm", "std.vm."),
        ("native-boundary", "std.native."),
    ]
    .iter()
    .find_map(|(feature, prefix)| {
        if unsupported_features.contains(*feature) && import.starts_with(prefix) {
            Some(format!("{import} rejected: unsupported {feature}"))
        } else {
            None
        }
    })
}

fn build_plan_diagnostics(rejected_imports: &[String], profile: &DeviceProfile) -> Vec<String> {
    let mut diagnostics = Vec::new();
    diagnostics.push(format!(
        "profile `{}` uses capability-driven planning with no host defaults",
        profile.name
    ));
    diagnostics.extend(rejected_imports.iter().cloned());
    diagnostics.push(format!(
        "profile `{}` emits diagnostics-only lowering prerequisites",
        profile.name
    ));
    diagnostics
}

fn validate_plan_no_path_leakage(plan: &DeviceTargetPlan) -> QualityResult<()> {
    let joined = [
        plan.std_subset.join("\n"),
        plan.package_capabilities.join("\n"),
        plan.native_bindings.join("\n"),
        plan.rejected_imports.join("\n"),
        plan.required_toolchains.join("\n"),
        plan.output_artifacts.join("\n"),
        plan.diagnostics.join("\n"),
    ]
    .join("\n");
    if joined.contains("/home/") || joined.contains("/tmp/") {
        return Err(render_failure(&[format!(
            "plan `{}` leaked a source checkout path",
            plan.profile_name
        )]));
    }
    Ok(())
}

fn write_report(
    report_path: &Path,
    profiles: &[DeviceProfile],
    plans: &[DeviceTargetPlan],
    rejected_features: &BTreeSet<String>,
    diagnostics: &BTreeSet<String>,
) -> QualityResult<()> {
    let report_entries = plans
        .iter()
        .flat_map(|plan| {
            [
                plan.profile_name.as_str(),
                plan.plan_hash.as_str(),
                plan.selected_runtime.as_str(),
                plan.memory_policy.as_str(),
            ]
        })
        .chain(rejected_features.iter().map(String::as_str))
        .chain(diagnostics.iter().map(String::as_str))
        .chain(FUTURE_LOWERING_PREREQUISITES.iter().copied())
        .collect::<Vec<_>>();
    validate_no_placeholders(&report_entries)?;

    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create device target planner report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.device-target-planner.v1",
        "profiles_checked": profiles.iter().map(profile_json).collect::<Vec<_>>(),
        "plan_hashes": plans.iter().map(|plan| json!({
            "profile": plan.profile_name,
            "hash": plan.plan_hash
        })).collect::<Vec<_>>(),
        "plans": plans.iter().map(plan_json).collect::<Vec<_>>(),
        "rejected_feature_list": rejected_features.iter().collect::<Vec<_>>(),
        "required_package_capabilities": plans.iter().flat_map(|plan| plan.package_capabilities.iter()).collect::<BTreeSet<_>>(),
        "diagnostics": diagnostics.iter().collect::<Vec<_>>(),
        "adversarial_cases": REQUIRED_ADVERSARIAL_CASES,
        "future_lowering_prerequisites": FUTURE_LOWERING_PREREQUISITES
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize device target planner report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write device target planner report: {err}",
            report_path.display()
        )
    })
}

fn profile_json(profile: &DeviceProfile) -> Value {
    json!({
        "name": profile.name,
        "cpu": profile.cpu,
        "memory_budget_bytes": profile.memory_budget_bytes,
        "allocator_policy": profile.allocator_policy,
        "panic_strategy": profile.panic_strategy,
        "runtime_profile": profile.runtime_profile,
        "peripherals": profile.peripherals,
        "package_hal_capabilities": profile.package_hal_capabilities,
        "linker_output_format": profile.linker_output_format,
        "rust_target": profile.rust_target,
        "unsupported_terlan_features": profile.unsupported_terlan_features,
        "producible_artifacts": profile.producible_artifacts
    })
}

fn plan_json(plan: &DeviceTargetPlan) -> Value {
    json!({
        "profile_name": plan.profile_name,
        "selected_runtime": plan.selected_runtime,
        "std_subset": plan.std_subset,
        "package_capabilities": plan.package_capabilities,
        "native_bindings": plan.native_bindings,
        "memory_policy": plan.memory_policy,
        "rejected_imports": plan.rejected_imports,
        "required_toolchains": plan.required_toolchains,
        "output_artifacts": plan.output_artifacts,
        "diagnostics": plan.diagnostics,
        "plan_hash": plan.plan_hash
    })
}

fn validate_no_placeholders(entries: &[&str]) -> QualityResult<()> {
    let diagnostics = entries
        .iter()
        .flat_map(|entry| {
            let normalized = entry.to_ascii_lowercase();
            PLACEHOLDER_TERMS
                .iter()
                .filter(move |term| normalized.contains(**term))
                .map(move |term| {
                    format!(
                        "placeholder device target planner evidence `{entry}` contains `{term}`"
                    )
                })
        })
        .collect::<Vec<_>>();
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(())
}

fn required_string(object: &serde_json::Map<String, Value>, field: &str) -> QualityResult<String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            render_failure(&[format!(
                "device profile field `{field}` must be a non-empty string"
            )])
        })
}

fn required_u64(object: &serde_json::Map<String, Value>, field: &str) -> QualityResult<u64> {
    object.get(field).and_then(Value::as_u64).ok_or_else(|| {
        render_failure(&[format!(
            "device profile field `{field}` must be an unsigned integer"
        )])
    })
}

fn required_string_set(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> QualityResult<BTreeSet<String>> {
    let values = object.get(field).and_then(Value::as_array).ok_or_else(|| {
        render_failure(&[format!("device profile field `{field}` must be a list")])
    })?;
    let mut set = BTreeSet::new();
    for value in values {
        let text = value
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                render_failure(&[format!(
                    "device profile field `{field}` contains a non-string value"
                )])
            })?;
        set.insert(text.to_string());
    }
    Ok(set)
}

fn stable_plan_hash(input: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[device-target-planner] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "device_target_planner_test.rs"]
mod device_target_planner_test;
