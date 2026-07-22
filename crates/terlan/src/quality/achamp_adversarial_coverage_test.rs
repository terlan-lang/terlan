use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

#[test]
fn operation_inventory_normalizes_generic_method_parameters() {
    assert_eq!(
        extract_operation_name("visit_retained_entries<'a>(&'a self)"),
        Some("visit_retained_entries".to_string())
    );
}

/// Verifies the A-CHAMP gate accepts a complete fixture.
///
/// Inputs:
/// - Fixture files containing every required layout test, value test, and
///   active source fragment.
///
/// Output:
/// - Summary counts equal to the gate's required anchor counts.
///
/// Transformation:
/// - Locks the quality gate contract without depending on the full repository
///   map implementation.
#[test]
fn achamp_adversarial_coverage_accepts_complete_fixture() {
    let root = temp_repo("achamp_adversarial_coverage_accepts");
    write_complete_fixture(&root);

    let summary = run_achamp_adversarial_coverage(&root).expect("fixture should pass");

    assert_eq!(summary.layout_test_count, REQUIRED_LAYOUT_TESTS.len());
    assert_eq!(summary.value_test_count, REQUIRED_VALUE_TESTS.len());
    assert_eq!(summary.node_variant_count, REQUIRED_NODE_VARIANTS.len());
    assert_eq!(summary.map_operation_count, REQUIRED_MAP_OPERATIONS.len());
    assert_eq!(
        summary.source_fragment_count,
        REQUIRED_SOURCE_FRAGMENTS.len()
    );
    assert_eq!(
        summary.randomized_backend_guard_count,
        DISALLOWED_RANDOMIZED_BACKENDS.len()
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies missing layout-node coverage is rejected.
///
/// Inputs:
/// - Fixture with one required map-layout test anchor removed.
///
/// Output:
/// - Diagnostic naming the missing layout test.
///
/// Transformation:
/// - Prevents A-CHAMP node-family coverage from silently disappearing.
#[test]
fn achamp_adversarial_coverage_rejects_missing_layout_test() {
    let root = temp_repo("achamp_adversarial_coverage_missing_layout");
    write_complete_fixture(&root);
    write(
        &root,
        MAP_LAYOUT_TEST,
        &test_file(
            REQUIRED_LAYOUT_TESTS
                .iter()
                .copied()
                .filter(|name| *name != "achamp_node_layout_prioritizes_true_hash_collisions"),
        ),
    );

    let error =
        run_achamp_adversarial_coverage(&root).expect_err("missing layout test should fail");

    assert!(error.contains(
        "missing required A-CHAMP test anchor `achamp_node_layout_prioritizes_true_hash_collisions`"
    ));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies missing active-map behavior coverage is rejected.
///
/// Inputs:
/// - Fixture with one required active map-value test anchor removed.
///
/// Output:
/// - Diagnostic naming the missing value test.
///
/// Transformation:
/// - Keeps mutation and persistent-update semantics tied to direct tests.
#[test]
fn achamp_adversarial_coverage_rejects_missing_value_test() {
    let root = temp_repo("achamp_adversarial_coverage_missing_value");
    write_complete_fixture(&root);
    write(
        &root,
        MAP_VALUE_TEST,
        &test_file(
            REQUIRED_VALUE_TESTS.iter().copied().filter(|name| {
                *name != "indexed_map_remove_then_reinsert_restores_length_and_value"
            }),
        ),
    );

    let error = run_achamp_adversarial_coverage(&root).expect_err("missing value test should fail");

    assert!(error.contains(
        "missing required A-CHAMP test anchor `indexed_map_remove_then_reinsert_restores_length_and_value`"
    ));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies missing active implementation fragments are rejected.
///
/// Inputs:
/// - Fixture source missing one required A-CHAMP node variant fragment.
///
/// Output:
/// - Diagnostic naming the missing source fragment.
///
/// Transformation:
/// - Prevents dormant design-only tests from satisfying the active runtime
///   coverage gate.
#[test]
fn achamp_adversarial_coverage_rejects_missing_source_fragment() {
    let root = temp_repo("achamp_adversarial_coverage_missing_source");
    write_complete_fixture(&root);
    write(
        &root,
        MAP_VALUE_SOURCE,
        &source_file(
            REQUIRED_NODE_VARIANTS.iter().copied(),
            REQUIRED_MAP_OPERATIONS.iter().copied(),
            REQUIRED_SOURCE_FRAGMENTS
                .iter()
                .copied()
                .filter(|fragment| *fragment != "AChampNode::CompressedPathNode"),
        ),
    );

    let error =
        run_achamp_adversarial_coverage(&root).expect_err("missing source fragment should fail");

    assert!(
        error.contains("missing required A-CHAMP source fragment `AChampNode::CompressedPathNode`")
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies new node variants require explicit coverage inventory updates.
///
/// Inputs:
/// - Fixture source with an extra `BitmapNode` variant.
///
/// Output:
/// - Diagnostic naming the unexpected node variant.
///
/// Transformation:
/// - Makes dormant or partially-covered A-CHAMP node additions impossible to
///   merge through this gate.
#[test]
fn achamp_adversarial_coverage_rejects_uninventoried_node_variant() {
    let root = temp_repo("achamp_adversarial_coverage_extra_variant");
    write_complete_fixture(&root);
    write(
        &root,
        MAP_VALUE_SOURCE,
        &source_file(
            REQUIRED_NODE_VARIANTS
                .iter()
                .copied()
                .chain(std::iter::once("BitmapNode")),
            REQUIRED_MAP_OPERATIONS.iter().copied(),
            REQUIRED_SOURCE_FRAGMENTS.iter().copied(),
        ),
    );

    let error = run_achamp_adversarial_coverage(&root).expect_err("extra variant should fail");

    assert!(error.contains("unexpected A-CHAMP enum `AChampNode` variant `BitmapNode`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies new public map operations require explicit coverage updates.
///
/// Inputs:
/// - Fixture source with an extra `merge` operation.
///
/// Output:
/// - Diagnostic naming the unexpected operation.
///
/// Transformation:
/// - Forces active public map behavior to be added to the coverage inventory.
#[test]
fn achamp_adversarial_coverage_rejects_uninventoried_map_operation() {
    let root = temp_repo("achamp_adversarial_coverage_extra_operation");
    write_complete_fixture(&root);
    write(
        &root,
        MAP_VALUE_SOURCE,
        &source_file(
            REQUIRED_NODE_VARIANTS.iter().copied(),
            REQUIRED_MAP_OPERATIONS
                .iter()
                .copied()
                .chain(std::iter::once("merge")),
            REQUIRED_SOURCE_FRAGMENTS.iter().copied(),
        ),
    );

    let error = run_achamp_adversarial_coverage(&root).expect_err("extra operation should fail");

    assert!(error.contains("unexpected A-CHAMP map operation on `VmMapValue` `merge`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies randomized map backends are rejected for VM-owned maps.
///
/// Inputs:
/// - Fixture source with a `HashMap` backend fragment.
///
/// Output:
/// - Diagnostic naming the disallowed randomized backend.
///
/// Transformation:
/// - Prevents Rust `HashMap` randomization from becoming part of Terlan map
///   semantics through implementation drift.
#[test]
fn achamp_adversarial_coverage_rejects_randomized_map_backend() {
    let root = temp_repo("achamp_adversarial_coverage_randomized_backend");
    write_complete_fixture(&root);
    let mut source = source_file(
        REQUIRED_NODE_VARIANTS.iter().copied(),
        REQUIRED_MAP_OPERATIONS.iter().copied(),
        REQUIRED_SOURCE_FRAGMENTS.iter().copied(),
    );
    source.push_str("\nuse std::collections::HashMap;\n");
    write(&root, MAP_VALUE_SOURCE, &source);

    let error = run_achamp_adversarial_coverage(&root).expect_err("HashMap backend should fail");

    assert!(error.contains("disallowed randomized map backend fragment `HashMap`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Writes a complete gate fixture.
fn write_complete_fixture(root: &Path) {
    write(
        root,
        MAP_LAYOUT_TEST,
        &test_file(REQUIRED_LAYOUT_TESTS.iter().copied()),
    );
    write(
        root,
        MAP_VALUE_TEST,
        &test_file(REQUIRED_VALUE_TESTS.iter().copied()),
    );
    write(
        root,
        MAP_VALUE_SOURCE,
        &source_file(
            REQUIRED_NODE_VARIANTS.iter().copied(),
            REQUIRED_MAP_OPERATIONS.iter().copied(),
            REQUIRED_SOURCE_FRAGMENTS.iter().copied(),
        ),
    );
}

/// Renders named test anchors.
fn test_file<'a>(names: impl Iterator<Item = &'a str>) -> String {
    names
        .map(|name| format!("#[test]\nfn {name}() {{}}\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Renders source fragments.
fn source_fragments<'a>(fragments: impl Iterator<Item = &'a str>) -> String {
    fragments.collect::<Vec<_>>().join("\n")
}

/// Renders an active map source fixture.
fn source_file<'a>(
    variants: impl Iterator<Item = &'a str>,
    operations: impl Iterator<Item = &'a str>,
    fragments: impl Iterator<Item = &'a str>,
) -> String {
    let variant_text = variants
        .map(|variant| format!("    {variant},"))
        .collect::<Vec<_>>()
        .join("\n");
    let operation_text = operations
        .map(|operation| format!("    pub(crate) fn {operation}() {{}}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "pub(crate) enum AChampNode {{\n{variant_text}\n}}\n\
         impl<K, V> VmMapValue<K, V> {{\n{operation_text}\n}}\n{}",
        source_fragments(fragments)
    )
}

/// Writes a fixture file, creating parents first.
fn write(root: &Path, relative_path: &str, text: &str) {
    let path = root.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(path, text).expect("write fixture file");
}

/// Creates a unique temporary repository root.
fn temp_repo(label: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("{label}_{}_{}", std::process::id(), now));
    fs::create_dir_all(&root).expect("create temp repo");
    root
}
