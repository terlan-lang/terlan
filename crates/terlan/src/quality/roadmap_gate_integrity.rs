use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::terlan_quality::{render_failure, QualityResult};

const ROADMAP_RELATIVE_PATH: &str = "docs/roadmap/ROADMAP_0_0_7.md";
const IMPLEMENTED_ARCHIVE_FILE: &str = "archive/ROADMAP_0_0_7_IMPLEMENTED.md";
const MULTICORE_ROADMAP_FILE: &str = "ROADMAP_0_0_7_MULTICORE_VM.md";
const PLANNED_GATES_HEADING: &str = "## Planned Gates";
const MULTICORE_GATES_HEADING: &str = "## Complete Multicore Gate Set";
const MAKE_FILES: &[&str] = &[
    "Makefile",
    "crates/terlan/cli.mk",
    "std/stdlib.mk",
    "editors/editor.mk",
];
const COMPLETED_RUST_GATES_VARIABLE: &str = "COMPLETED_SLICE_RUST_GATES";

/// Summary produced by the roadmap gate integrity check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoadmapGateIntegritySummary {
    pub planned_gate_count: usize,
    pub unchecked_slice_count: usize,
    pub make_target_count: usize,
}

/// Runs the roadmap gate integrity check.
///
/// Inputs:
/// - Repository root containing the Make graph.
/// - The active 0.0.7 roadmap either in this repository or the parent
///   documentation workspace.
///
/// Output:
/// - Success summary when active roadmap gates are executable or explicitly
///   still owned by unchecked slices.
/// - Stable diagnostics when completed slices reference missing gates, planned
///   gates are unowned, or unchecked slices lack executable acceptance detail.
///
/// Transformation:
/// - Turns the active roadmap into a consistency contract so future roadmap
///   edits cannot add vague, unwired checklist items.
pub fn run_roadmap_gate_integrity(root: &Path) -> QualityResult<RoadmapGateIntegritySummary> {
    let roadmap_path = find_roadmap(root)?;
    let roadmap = fs::read_to_string(&roadmap_path)
        .map_err(|err| format!("{}: failed to read roadmap: {err}", roadmap_path.display()))?;
    let archive_path = roadmap_path
        .parent()
        .expect("roadmap path has a parent")
        .join(IMPLEMENTED_ARCHIVE_FILE);
    let implemented_archive = fs::read_to_string(&archive_path).map_err(|err| {
        format!(
            "{}: failed to read implemented roadmap archive: {err}",
            archive_path.display()
        )
    })?;
    let multicore_path = roadmap_path
        .parent()
        .expect("roadmap path has a parent")
        .join(MULTICORE_ROADMAP_FILE);
    let multicore_roadmap = fs::read_to_string(&multicore_path).map_err(|err| {
        format!(
            "{}: failed to read multicore roadmap: {err}",
            multicore_path.display()
        )
    })?;
    let make_targets = collect_make_targets(root)?;
    let planned_gate_order = parse_gate_block(&roadmap, PLANNED_GATES_HEADING);
    let planned_gates = planned_gate_order.iter().cloned().collect::<BTreeSet<_>>();
    let multicore_gate_order = parse_gate_block(&multicore_roadmap, MULTICORE_GATES_HEADING);
    let unchecked_slices = parse_roadmap_slices(&roadmap, false);
    let active_completed_slices = parse_roadmap_slices(&roadmap, true);
    let archived_completed_slices = parse_roadmap_slices(&implemented_archive, true);
    let make_graph = read_make_graph(root)?;
    let mut diagnostics = Vec::new();
    validate_quality_enforcement_rule(&roadmap, &mut diagnostics);
    validate_inventory_snapshot(
        &roadmap,
        planned_gates.len(),
        unchecked_slices.len(),
        make_targets.len(),
        &mut diagnostics,
    );
    validate_multicore_gate_sync(&planned_gate_order, &multicore_gate_order, &mut diagnostics);
    diagnostics.extend(validate_roadmap_gate_integrity(
        &planned_gates,
        &unchecked_slices,
        &active_completed_slices,
        &make_targets,
    ));
    diagnostics.extend(validate_completed_slice_rust_ownership(
        &archived_completed_slices,
        &unchecked_slices,
        &make_graph,
        &make_targets,
    ));
    if !diagnostics.is_empty() {
        return Err(render_failure("roadmap-gate-integrity", &diagnostics));
    }
    Ok(RoadmapGateIntegritySummary {
        planned_gate_count: planned_gates.len(),
        unchecked_slice_count: unchecked_slices.len(),
        make_target_count: make_targets.len(),
    })
}

/// Reads all Make fragments that contribute release and roadmap targets.
fn read_make_graph(root: &Path) -> QualityResult<String> {
    let mut graph = String::new();
    for relative in MAKE_FILES {
        let path = root.join(relative);
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        graph.push_str(&text);
        graph.push('\n');
    }
    if graph.trim().is_empty() {
        return Err("no Make graph found for roadmap gate integrity".to_string());
    }
    Ok(graph)
}

/// Finds the active roadmap from the compiler root.
fn find_roadmap(root: &Path) -> QualityResult<PathBuf> {
    let root = root
        .canonicalize()
        .map_err(|err| format!("failed to canonicalize repository root: {err}"))?;
    let local = root.join(ROADMAP_RELATIVE_PATH);
    if local.exists() {
        return Ok(local);
    }
    let parent = root
        .parent()
        .map(|parent| parent.join(ROADMAP_RELATIVE_PATH))
        .ok_or_else(|| "repository root has no parent for roadmap lookup".to_string())?;
    if parent.exists() {
        return Ok(parent);
    }
    Err(format!(
        "missing active roadmap at `{}` or `{}`",
        local.display(),
        parent.display()
    ))
}

/// Collects Make targets from the root Make graph files.
fn collect_make_targets(root: &Path) -> QualityResult<BTreeSet<String>> {
    let target_re = Regex::new(r"^([A-Za-z0-9_.-]+)\s*:").expect("valid Make target regex");
    let mut targets = BTreeSet::new();
    for relative in MAKE_FILES {
        let path = root.join(relative);
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('.') || trimmed.contains(":=") || trimmed.contains("?=") {
                continue;
            }
            if let Some(captures) = target_re.captures(trimmed) {
                targets.insert(captures[1].to_string());
            }
        }
    }
    if targets.is_empty() {
        return Err("no Make targets found for roadmap gate integrity".to_string());
    }
    Ok(targets)
}

/// Parses one roadmap gate section while preserving declaration order.
///
/// Inputs:
/// - Complete roadmap text.
/// - Exact level-two heading that owns a fenced Make command inventory.
///
/// Output:
/// - Every first target following `make`, in declaration order.
///
/// Transformation:
/// - Retains ordering and duplicates so cross-roadmap contracts can reject
///   drift that an unordered set would hide.
fn parse_gate_block(roadmap: &str, heading: &str) -> Vec<String> {
    let mut in_planned = false;
    let mut in_fence = false;
    let mut gates = Vec::new();
    for line in roadmap.lines() {
        if line.trim() == heading {
            in_planned = true;
            continue;
        }
        if in_planned && line.starts_with("## ") && line.trim() != heading {
            break;
        }
        if !in_planned {
            continue;
        }
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            if let Some(rest) = line.trim().strip_prefix("make ") {
                if let Some(name) = rest.split_whitespace().next() {
                    gates.push(name.to_string());
                }
            }
        }
    }
    gates
}

/// Validates the multicore closeout inventory is canonical in the main roadmap.
///
/// Inputs:
/// - Ordered main-roadmap planned gates.
/// - Ordered multicore mini-roadmap complete gate set.
/// - Mutable diagnostic collection.
///
/// Output:
/// - No diagnostics only when both inventories are duplicate-free and the
///   complete multicore sequence appears contiguously in the main inventory.
///
/// Transformation:
/// - Makes the mini-roadmap an executable source of truth while ensuring the
///   release roadmap cannot omit, reorder, or partially copy its closeout.
fn validate_multicore_gate_sync(
    planned_gates: &[String],
    multicore_gates: &[String],
    diagnostics: &mut Vec<String>,
) {
    if multicore_gates.is_empty() {
        diagnostics.push(format!(
            "`{MULTICORE_GATES_HEADING}` contains no `make ...` commands"
        ));
        return;
    }
    report_duplicate_gates("main planned gate inventory", planned_gates, diagnostics);
    report_duplicate_gates(
        "multicore complete gate inventory",
        multicore_gates,
        diagnostics,
    );

    if planned_gates
        .windows(multicore_gates.len())
        .any(|window| window == multicore_gates)
    {
        return;
    }

    let planned = planned_gates.iter().collect::<BTreeSet<_>>();
    let missing = multicore_gates
        .iter()
        .filter(|gate| !planned.contains(gate))
        .cloned()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        diagnostics.push(
            "main planned gate inventory must contain the complete multicore gate set in exact contiguous order"
                .to_string(),
        );
    } else {
        diagnostics.push(format!(
            "main planned gate inventory is missing multicore gates: {}",
            missing.join(", ")
        ));
    }
}

/// Reports repeated gate commands that would make roadmap ownership ambiguous.
fn report_duplicate_gates(label: &str, gates: &[String], diagnostics: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for gate in gates {
        if !seen.insert(gate) {
            duplicates.insert(gate);
        }
    }
    if !duplicates.is_empty() {
        diagnostics.push(format!(
            "{label} contains duplicate gates: {}",
            duplicates
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}

/// Parses active roadmap checklist slices.
fn parse_roadmap_slices(roadmap: &str, completed: bool) -> Vec<RoadmapSlice> {
    let marker = if completed { "- [x] " } else { "- [ ] " };
    let mut slices = Vec::new();
    let mut current: Option<RoadmapSlice> = None;
    for line in roadmap.lines() {
        if let Some(title) = line.strip_prefix(marker) {
            if let Some(slice) = current.take() {
                slices.push(slice);
            }
            current = Some(RoadmapSlice {
                title: title.trim().to_string(),
                body: String::new(),
            });
            continue;
        }
        if line.starts_with("- [x] ")
            || line.starts_with("- [ ] ")
            || line.starts_with("## ")
            || line.starts_with("### ")
        {
            if let Some(slice) = current.take() {
                slices.push(slice);
            }
        }
        if let Some(slice) = &mut current {
            slice.body.push_str(line);
            slice.body.push('\n');
        }
    }
    if let Some(slice) = current {
        slices.push(slice);
    }
    slices
}

/// Validates planned gates and checklist slice detail.
fn validate_roadmap_gate_integrity(
    planned_gates: &BTreeSet<String>,
    unchecked_slices: &[RoadmapSlice],
    completed_slices: &[RoadmapSlice],
    make_targets: &BTreeSet<String>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if planned_gates.is_empty() {
        diagnostics.push("planned gates block contains no `make ...` commands".to_string());
    }
    let unchecked_gate_owners = collect_slice_gates(unchecked_slices);
    let completed_gate_owners = collect_slice_gates(completed_slices);
    for gate in planned_gates {
        if !make_targets.contains(gate) && !unchecked_gate_owners.contains(gate) {
            diagnostics.push(format!(
                "planned gate `{gate}` is missing from Make graph and no unchecked slice owns it"
            ));
        }
    }
    for gate in completed_gate_owners {
        if !make_targets.contains(&gate) {
            diagnostics.push(format!(
                "completed roadmap slice references missing Make target `{gate}`"
            ));
        }
    }
    for slice in unchecked_slices {
        validate_unchecked_slice(slice, &mut diagnostics);
        validate_slice_completion_evidence(slice, &mut diagnostics);
    }
    for slice in completed_slices {
        validate_slice_completion_evidence(slice, &mut diagnostics);
    }
    diagnostics
}

/// Ensures completed Rust slices are owned by the canonical suite exactly once.
fn validate_completed_slice_rust_ownership(
    completed_slices: &[RoadmapSlice],
    unchecked_slices: &[RoadmapSlice],
    make_graph: &str,
    make_targets: &BTreeSet<String>,
) -> Vec<String> {
    let completed = collect_slice_gates(completed_slices);
    let unchecked = collect_slice_gates(unchecked_slices);
    let completed_only = completed
        .difference(&unchecked)
        .cloned()
        .collect::<BTreeSet<_>>();
    let owners = parse_make_list_variable(make_graph, COMPLETED_RUST_GATES_VARIABLE);
    let recipes = parse_make_target_recipes(make_graph);
    let mut diagnostics = Vec::new();

    for gate in &owners {
        if !completed_only.contains(gate) {
            diagnostics.push(format!(
                "canonical completed-slice Rust owner lists non-completed gate `{gate}`"
            ));
        }
        if !make_targets.contains(gate) {
            diagnostics.push(format!(
                "canonical completed-slice Rust owner lists missing Make target `{gate}`"
            ));
        }
    }
    for gate in completed_only {
        let has_bespoke_rust_test = recipes.get(&gate).is_some_and(|lines| {
            lines.iter().any(|line| {
                [
                    "$(RUST_TEST)",
                    "$(EXACT_CARGO_TEST)",
                    "$(TERLC_EXACT_TEST)",
                    "$(CARGO) test",
                ]
                .iter()
                .any(|marker| line.contains(marker))
            })
        });
        if has_bespoke_rust_test {
            diagnostics.push(format!(
                "completed roadmap gate `{gate}` owns bespoke Rust tests; move them to `rust-test-suite` and list the gate in `{COMPLETED_RUST_GATES_VARIABLE}`"
            ));
        }
    }
    diagnostics
}

/// Parses a continued Make list variable into a deduplicated value set.
fn parse_make_list_variable(make_graph: &str, variable: &str) -> BTreeSet<String> {
    parse_make_list_variable_values(make_graph, variable)
        .into_iter()
        .collect()
}

/// Parses a continued Make list variable while preserving declaration order.
pub(super) fn parse_make_list_variable_values(make_graph: &str, variable: &str) -> Vec<String> {
    let prefix = format!("{variable} :=");
    let mut values = Vec::new();
    let mut collecting = false;
    for line in make_graph.lines() {
        let trimmed = line.trim();
        if !collecting {
            let Some(rest) = trimmed.strip_prefix(&prefix) else {
                continue;
            };
            collecting = true;
            for value in rest.trim_end_matches('\\').split_whitespace() {
                values.push(value.to_string());
            }
            if !trimmed.ends_with('\\') {
                break;
            }
            continue;
        }
        for value in trimmed.trim_end_matches('\\').split_whitespace() {
            values.push(value.to_string());
        }
        if !trimmed.ends_with('\\') {
            break;
        }
    }
    values
}

/// Collects normalized recipe lines for concrete Make targets.
fn parse_make_target_recipes(make_graph: &str) -> BTreeMap<String, Vec<String>> {
    let target_re = Regex::new(r"^([A-Za-z0-9_.-]+)\s*:").expect("valid Make target regex");
    let mut recipes = BTreeMap::<String, Vec<String>>::new();
    let mut current_target = None;
    for line in make_graph.lines() {
        if let Some(captures) = target_re.captures(line) {
            current_target = Some(captures[1].to_string());
            continue;
        }
        if let Some(recipe) = line.strip_prefix('\t') {
            if let Some(target) = &current_target {
                recipes
                    .entry(target.clone())
                    .or_default()
                    .push(recipe.trim().to_string());
            }
        } else if !line.trim().is_empty() && !line.trim_start().starts_with('#') {
            current_target = None;
        }
    }
    recipes
}

/// Validates the global quality enforcement contract remains present.
///
/// Inputs:
/// - Full active roadmap text.
///
/// Output:
/// - Diagnostics when the roadmap drops non-negotiable quality requirements.
///
/// Transformation:
/// - Keeps the roadmap from regressing into checklist-only progress by making
///   real behavior tests, adversarial coverage, file-size hygiene, and
///   code-smell inspection mandatory text owned by the gate.
fn validate_quality_enforcement_rule(roadmap: &str, diagnostics: &mut Vec<String>) {
    let Some(section) = roadmap_section(roadmap, "## Quality Enforcement Rule") else {
        diagnostics.push("missing top-level `Quality Enforcement Rule` section".to_string());
        return;
    };
    let normalized = normalize_whitespace(section);
    for required in [
        "real behavior",
        "intended user path",
        "adversarial tests",
        "table-driven",
        "property-based",
        "skipped/unsupported manifest",
        "file-size",
        "rust-quality-check",
        "code smells",
    ] {
        if !normalized.contains(required) {
            diagnostics.push(format!(
                "`Quality Enforcement Rule` must mention `{required}`"
            ));
        }
    }
}

/// Validates the published roadmap-gate inventory count.
///
/// Inputs:
/// - Full active roadmap text.
/// - Parsed gate, unchecked-slice, and Make-target counts.
///
/// Output:
/// - Diagnostic when the prose inventory count is absent or stale.
///
/// Transformation:
/// - Keeps the roadmap's current-state summary synchronized with the
///   executable gate output.
fn validate_inventory_snapshot(
    roadmap: &str,
    planned_gate_count: usize,
    unchecked_slice_count: usize,
    make_target_count: usize,
    diagnostics: &mut Vec<String>,
) {
    let Some(section) = roadmap_section(roadmap, "### Roadmap Gate Integrity") else {
        diagnostics.push("missing `Roadmap Gate Integrity` section".to_string());
        return;
    };
    let normalized = normalize_whitespace(section);
    let expected = format!(
        "Current validated inventory: {planned_gate_count} planned gates, \
         {unchecked_slice_count} unchecked slices, and {make_target_count} Make targets."
    );
    if !normalized.contains(&normalize_whitespace(&expected)) {
        diagnostics.push(format!(
            "`Roadmap Gate Integrity` current inventory must report `{}`",
            normalize_whitespace(&expected)
        ));
    }
}

/// Collapses all whitespace runs into single spaces.
fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Returns a top-level roadmap section by heading.
fn roadmap_section<'a>(roadmap: &'a str, heading: &str) -> Option<&'a str> {
    let start = roadmap.find(heading)?;
    let rest = &roadmap[start..];
    let next = rest
        .lines()
        .enumerate()
        .skip(1)
        .find_map(|(line_index, line)| {
            if line.starts_with("## ") {
                let byte_offset = rest
                    .lines()
                    .take(line_index)
                    .map(|line| line.len() + 1)
                    .sum::<usize>();
                Some(byte_offset)
            } else {
                None
            }
        })
        .unwrap_or(rest.len());
    Some(&rest[..next])
}

/// Collects gates named by roadmap slices.
fn collect_slice_gates(slices: &[RoadmapSlice]) -> BTreeSet<String> {
    let gate_re = Regex::new(r"`make ([A-Za-z0-9_.-]+)").expect("valid gate regex");
    let mut gates = BTreeSet::new();
    for slice in slices {
        for line in slice.body.lines() {
            let lower = line.to_lowercase();
            if !lower.contains("gate:") && !lower.contains("guard to keep") {
                continue;
            }
            for captures in gate_re.captures_iter(line) {
                gates.insert(captures[1].to_string());
            }
        }
    }
    gates
}

/// Validates one unchecked roadmap slice.
fn validate_unchecked_slice(slice: &RoadmapSlice, diagnostics: &mut Vec<String>) {
    let normalized = slice.body.to_lowercase();
    if !normalized.contains("gate:") {
        diagnostics.push(format!("unchecked slice `{}` has no gate", slice.title));
    }
    if !normalized.contains("acceptance:") {
        diagnostics.push(format!(
            "unchecked slice `{}` has no acceptance criteria",
            slice.title
        ));
    }
    let has_positive_executable_test = normalized.contains("positive")
        || normalized.contains("executable")
        || normalized.contains(" test")
        || normalized.contains("tests")
        || normalized.contains("fixture")
        || normalized.contains("fixtures")
        || normalized.contains("validate")
        || normalized.contains("validation")
        || normalized.contains("tests prove")
        || normalized.contains("test proves")
        || normalized.contains("prove")
        || normalized.contains("proves")
        || normalized.contains("property")
        || normalized.contains("benchmark")
        || normalized.contains("coverage")
        || normalized.contains("report")
        || normalized.contains("roundtrip")
        || normalized.contains("round-trip");
    if !has_positive_executable_test {
        diagnostics.push(format!(
            "unchecked slice `{}` lacks positive executable test language",
            slice.title
        ));
    }
    let has_adversarial = normalized.contains("adversarial")
        || normalized.contains("stable diagnostic")
        || normalized.contains("diagnostics")
        || normalized.contains("diagnostic")
        || normalized.contains("reject")
        || normalized.contains("rejected")
        || normalized.contains("invalid")
        || normalized.contains("missing")
        || normalized.contains("stale")
        || normalized.contains("failure")
        || normalized.contains("rollback")
        || normalized.contains("must fail")
        || normalized.contains("gate fails")
        || normalized.contains("fails if");
    if !has_adversarial {
        diagnostics.push(format!(
            "unchecked slice `{}` lacks adversarial or stable-failure coverage language",
            slice.title
        ));
    }
}

/// Validates a slice does not treat weak markers as completion evidence.
///
/// Inputs:
/// - Active or completed roadmap slice.
///
/// Output:
/// - Diagnostics for gate, acceptance, or current-state lines that claim weak
///   evidence such as symbol existence or marker-only checks.
///
/// Transformation:
/// - Enforces the quality rule at roadmap-review time instead of relying on
///   reviewer memory.
fn validate_slice_completion_evidence(slice: &RoadmapSlice, diagnostics: &mut Vec<String>) {
    for line in slice.body.lines() {
        let normalized = line.trim().to_lowercase();
        let is_evidence_line = normalized.starts_with("- gate:")
            || normalized.starts_with("- acceptance:")
            || normalized.starts_with("- current gate state:")
            || normalized.starts_with("- current executable conversion state:");
        if !is_evidence_line {
            continue;
        }
        for phrase in [
            "source files exist",
            "generated files exist",
            "strings appear",
            "marker check",
            "marker-only",
            "declaration-only",
            "declaration typechecks",
            "symbol exists",
            "assert(true)",
            "assert_equal(x, x)",
            "identity assertion",
            "surface is declared",
            "is declared",
        ] {
            let is_rejection_language = normalized.contains("reject")
                || normalized.contains("fail if")
                || normalized.contains("fails if")
                || normalized.contains("must fail");
            if normalized.contains(phrase) && !is_rejection_language {
                diagnostics.push(format!(
                    "slice `{}` uses weak completion evidence `{phrase}`",
                    slice.title
                ));
            }
        }
    }
}

/// Roadmap checklist slice body.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RoadmapSlice {
    title: String,
    body: String,
}

#[cfg(test)]
#[path = "roadmap_gate_integrity_test.rs"]
mod roadmap_gate_integrity_test;
