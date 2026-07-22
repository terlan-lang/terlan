use std::fs;
use std::path::Path;

use crate::terlan_quality::{render_failure, QualityResult};

const MAP_LAYOUT_TEST: &str = "crates/terlan/src/runtime/vm/map_layout_test.rs";
const MAP_VALUE_TEST: &str = "crates/terlan/src/runtime/vm/map_value_test.rs";
const MAP_VALUE_SOURCE: &str = "crates/terlan/src/runtime/vm/map_value.rs";

const REQUIRED_LAYOUT_TESTS: &[&str] = &[
    "map_root_layout_keeps_small_maps_flat_through_boundary",
    "map_root_layout_extends_flat_storage_for_shared_literal_shapes",
    "map_root_layout_does_not_keep_dynamic_dictionaries_flat",
    "achamp_node_layout_uses_leaf_blocks_for_small_subtrees",
    "achamp_node_layout_prioritizes_true_hash_collisions",
    "achamp_node_layout_compresses_long_shared_prefixes",
    "achamp_node_layout_splits_sparse_and_dense_nodes",
];

const REQUIRED_VALUE_TESTS: &[&str] = &[
    "adaptive_map_switches_after_benchmarked_inflection_point",
    "indexed_map_lookup_replace_and_order_are_stable",
    "indexed_map_owned_private_update_reuses_unique_base_storage",
    "indexed_map_mutable_insert_extends_unique_base_without_patch_chain",
    "indexed_map_insert_and_remove_keep_bucket_lookup_coherent",
    "indexed_map_repeated_remove_does_not_decrement_len_twice",
    "indexed_map_remove_then_reinsert_restores_length_and_value",
    "indexed_map_shared_persistent_updates_compact_patch_chain",
    "indexed_map_shared_removes_compact_patch_chain",
    "indexed_map_clear_drops_achamp_storage_and_resets_length",
    "retained_entry_visitor_covers_flat_base_patch_and_tombstone_storage",
    "achamp_indexed_map_uses_collision_node_for_equal_hashes",
    "achamp_indexed_map_compresses_long_shared_hash_prefixes",
];

const REQUIRED_NODE_VARIANTS: &[&str] = &[
    "LeafBlock",
    "SparseNode",
    "DenseNode",
    "CollisionNode",
    "CompressedPathNode",
];

const REQUIRED_MAP_OPERATIONS: &[&str] = &[
    "from_entries",
    "to_entries",
    "is_empty",
    "len",
    "visit_retained_entries",
    "lookup",
    "insert_or_replace",
    "put_persistent",
    "put_persistent_owned",
    "remove",
    "clear",
];

const REQUIRED_SOURCE_FRAGMENTS: &[&str] = &[
    "AChampNode::LeafBlock",
    "AChampNode::SparseNode",
    "AChampNode::DenseNode",
    "AChampNode::CollisionNode",
    "AChampNode::CompressedPathNode",
    "select_achamp_node_layout",
    "patch_depth_for_test",
];

const DISALLOWED_RANDOMIZED_BACKENDS: &[&str] =
    &["HashMap", "RandomState", "hashbrown::HashMap", "AHashMap"];

/// Summary produced by the A-CHAMP adversarial coverage gate.
///
/// Inputs:
/// - Number of required map-layout test anchors.
/// - Number of required active map-value test anchors.
/// - Number of required source fragments proving active A-CHAMP ownership.
///
/// Output:
/// - Stable count summary for `terlan-quality`.
///
/// Transformation:
/// - Treats A-CHAMP coverage as a structural inventory rather than relying on
///   broad test counts or prose in the roadmap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AChampAdversarialCoverageSummary {
    pub layout_test_count: usize,
    pub value_test_count: usize,
    pub node_variant_count: usize,
    pub map_operation_count: usize,
    pub source_fragment_count: usize,
    pub randomized_backend_guard_count: usize,
}

/// Runs the A-CHAMP adversarial coverage gate.
///
/// Inputs:
/// - `root`: repository root containing VM runtime sources and tests.
///
/// Output:
/// - Success when every required A-CHAMP node family, transition, and active
///   map mutation behavior has a named test anchor.
/// - Stable diagnostics naming missing anchors or source fragments.
///
/// Transformation:
/// - Pins the active large-map implementation to direct adversarial coverage
///   before the performance baseline can be treated as release evidence.
pub fn run_achamp_adversarial_coverage(
    root: &Path,
) -> QualityResult<AChampAdversarialCoverageSummary> {
    let layout_tests = read_file(root, MAP_LAYOUT_TEST)?;
    let value_tests = read_file(root, MAP_VALUE_TEST)?;
    let map_value_source = read_file(root, MAP_VALUE_SOURCE)?;
    let mut diagnostics = Vec::new();

    require_test_anchors(
        MAP_LAYOUT_TEST,
        &layout_tests,
        REQUIRED_LAYOUT_TESTS,
        &mut diagnostics,
    );
    require_test_anchors(
        MAP_VALUE_TEST,
        &value_tests,
        REQUIRED_VALUE_TESTS,
        &mut diagnostics,
    );
    require_source_fragments(
        MAP_VALUE_SOURCE,
        &map_value_source,
        REQUIRED_SOURCE_FRAGMENTS,
        &mut diagnostics,
    );
    reject_source_fragments(
        MAP_VALUE_SOURCE,
        &map_value_source,
        DISALLOWED_RANDOMIZED_BACKENDS,
        &mut diagnostics,
    );
    require_enum_inventory(
        MAP_VALUE_SOURCE,
        &map_value_source,
        "AChampNode",
        REQUIRED_NODE_VARIANTS,
        &mut diagnostics,
    );
    require_impl_operation_inventory(
        MAP_VALUE_SOURCE,
        &map_value_source,
        "VmMapValue",
        REQUIRED_MAP_OPERATIONS,
        &mut diagnostics,
    );

    if diagnostics.is_empty() {
        Ok(AChampAdversarialCoverageSummary {
            layout_test_count: REQUIRED_LAYOUT_TESTS.len(),
            value_test_count: REQUIRED_VALUE_TESTS.len(),
            node_variant_count: REQUIRED_NODE_VARIANTS.len(),
            map_operation_count: REQUIRED_MAP_OPERATIONS.len(),
            source_fragment_count: REQUIRED_SOURCE_FRAGMENTS.len(),
            randomized_backend_guard_count: DISALLOWED_RANDOMIZED_BACKENDS.len(),
        })
    } else {
        Err(render_failure("achamp-adversarial-coverage", &diagnostics))
    }
}

/// Reads a required repository file.
fn read_file(root: &Path, relative_path: &str) -> QualityResult<String> {
    let path = root.join(relative_path);
    fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read required file: {err}", path.display()))
}

/// Requires exact Rust test function anchors.
fn require_test_anchors(
    relative_path: &str,
    text: &str,
    required_names: &[&str],
    diagnostics: &mut Vec<String>,
) {
    for name in required_names {
        let needle = format!("fn {name}(");
        if !text.contains(&needle) {
            diagnostics.push(format!(
                "{relative_path}: missing required A-CHAMP test anchor `{name}`"
            ));
        }
    }
}

/// Requires source fragments that prove the tested implementation is active.
fn require_source_fragments(
    relative_path: &str,
    text: &str,
    required_fragments: &[&str],
    diagnostics: &mut Vec<String>,
) {
    for fragment in required_fragments {
        if !text.contains(fragment) {
            diagnostics.push(format!(
                "{relative_path}: missing required A-CHAMP source fragment `{fragment}`"
            ));
        }
    }
}

/// Rejects source fragments that would make VM map behavior nondeterministic.
fn reject_source_fragments(
    relative_path: &str,
    text: &str,
    disallowed_fragments: &[&str],
    diagnostics: &mut Vec<String>,
) {
    for fragment in disallowed_fragments {
        if text.contains(fragment) {
            diagnostics.push(format!(
                "{relative_path}: disallowed randomized map backend fragment `{fragment}`"
            ));
        }
    }
}

/// Requires the active enum variants to match the coverage inventory exactly.
fn require_enum_inventory(
    relative_path: &str,
    text: &str,
    enum_name: &str,
    required_variants: &[&str],
    diagnostics: &mut Vec<String>,
) {
    let Some(body) = extract_named_block(text, &format!("enum {enum_name}")) else {
        diagnostics.push(format!("{relative_path}: missing enum `{enum_name}`"));
        return;
    };
    let variants = extract_top_level_variants(body);
    require_exact_inventory(
        relative_path,
        &format!("A-CHAMP enum `{enum_name}` variant"),
        required_variants,
        &variants,
        diagnostics,
    );
}

/// Requires public VM map operations to match the coverage inventory exactly.
fn require_impl_operation_inventory(
    relative_path: &str,
    text: &str,
    type_name: &str,
    required_operations: &[&str],
    diagnostics: &mut Vec<String>,
) {
    let Some(body) = extract_named_block(text, &format!("impl<K, V> {type_name}<K, V>")) else {
        diagnostics.push(format!("{relative_path}: missing impl for `{type_name}`"));
        return;
    };
    let operations = body
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("pub(crate) fn "))
        .filter_map(extract_operation_name)
        .collect::<Vec<_>>();
    require_exact_inventory(
        relative_path,
        &format!("A-CHAMP map operation on `{type_name}`"),
        required_operations,
        &operations,
        diagnostics,
    );
}

/// Extracts a Rust method name without generic parameter syntax.
fn extract_operation_name(tail: &str) -> Option<String> {
    let (name, _) = tail.split_once('(')?;
    Some(
        name.split_once('<')
            .map_or(name, |(base, _)| base)
            .to_string(),
    )
}

/// Extracts the body of a Rust item whose header contains `needle`.
fn extract_named_block<'a>(text: &'a str, needle: &str) -> Option<&'a str> {
    let item_start = text.find(needle)?;
    let open_relative = text[item_start..].find('{')?;
    let open = item_start + open_relative;
    let mut depth = 0usize;
    for (offset, ch) in text[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let close = open + offset;
                    return Some(&text[open + 1..close]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Extracts top-level enum variant names from an enum body.
fn extract_top_level_variants(body: &str) -> Vec<String> {
    let mut variants = Vec::new();
    let mut depth = 0usize;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if depth == 0 {
            if let Some(variant) = parse_variant_name(trimmed) {
                variants.push(variant);
            }
        }
        for ch in trimmed.chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    variants
}

/// Parses one top-level enum variant name.
fn parse_variant_name(line: &str) -> Option<String> {
    let name = line
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    if name.is_empty() {
        return None;
    }
    let rest = line[name.len()..].trim_start();
    if rest.starts_with('(') || rest.starts_with('{') || rest.starts_with(',') {
        Some(name)
    } else {
        None
    }
}

/// Requires an exact string inventory.
fn require_exact_inventory(
    relative_path: &str,
    label: &str,
    required_items: &[&str],
    actual_items: &[String],
    diagnostics: &mut Vec<String>,
) {
    for item in required_items {
        if !actual_items.iter().any(|actual| actual == item) {
            diagnostics.push(format!(
                "{relative_path}: missing required {label} `{item}`"
            ));
        }
    }
    for item in actual_items {
        if !required_items.iter().any(|required| required == item) {
            diagnostics.push(format!("{relative_path}: unexpected {label} `{item}`"));
        }
    }
}

#[cfg(test)]
#[path = "achamp_adversarial_coverage_test.rs"]
mod achamp_adversarial_coverage_test;
