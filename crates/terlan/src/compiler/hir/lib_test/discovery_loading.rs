use super::super::{
    load_discovery_interfaces_for_symbol_from_file_set, load_imported_interfaces_from_file_set,
};
use crate::terlan_syntax::parse_module_as_syntax_output;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

/// Verifies import-closed interface discovery avoids unrelated std summaries.
#[test]
pub(super) fn imported_interface_loading_reads_exact_dependency_closure() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan_hir_import_closure_{}_{}",
        std::process::id(),
        nanos
    ));
    let source_dir = root.join("src/app");
    let summaries = root.join("std/summaries");
    fs::create_dir_all(&source_dir).expect("create source fixture");
    fs::create_dir_all(source_dir.join("std")).expect("create empty nested std fixture");
    fs::create_dir_all(&summaries).expect("create summaries fixture");
    let source_path = source_dir.join("Main.terl");
    let source = "module app.Main.\n\nimport std.demo.Provider.{run}.\n";
    fs::write(&source_path, source).expect("write source fixture");
    fs::write(
        summaries.join("std.demo.Provider.typi"),
        "module std.demo.Provider.\n\npub run(): Int.\n",
    )
    .expect("write provider summary");
    fs::write(
        summaries.join("std.demo.Provider.typi.deps"),
        "deps=1\nstd.demo.Dependency=1\n",
    )
    .expect("write provider dependency manifest");
    fs::write(
        summaries.join("std.demo.Dependency.typi"),
        "module std.demo.Dependency.\n\npub type Value = Int.\n",
    )
    .expect("write dependency summary");
    fs::write(
        summaries.join("std.demo.Unrelated.typi"),
        "module std.demo.Unrelated.\n\npub type Noise = Int.\n",
    )
    .expect("write unrelated summary");

    let module = parse_module_as_syntax_output(source).expect("parse source fixture");
    let interfaces = load_imported_interfaces_from_file_set(
        source_path
            .to_str()
            .expect("temporary source path should be utf-8"),
        &module,
    );
    assert!(interfaces.contains_key("std.demo.Provider"));
    assert!(interfaces.contains_key("std.demo.Dependency"));
    assert!(!interfaces.contains_key("std.demo.Unrelated"));

    let discovered = load_discovery_interfaces_for_symbol_from_file_set(
        source_path
            .to_str()
            .expect("temporary source path should be utf-8"),
        "Noise",
    );
    assert!(discovered.contains_key("std.demo.Unrelated"));
    assert!(!discovered.contains_key("std.demo.Provider"));
    assert!(!discovered.contains_key("std.demo.Dependency"));
    fs::write(
        summaries.join("std.demo.Unrelated.typi"),
        "module std.demo.Refreshed.\n\npub type FreshNoise = Int.\n",
    )
    .expect("refresh unrelated summary");
    let refreshed = load_discovery_interfaces_for_symbol_from_file_set(
        source_path
            .to_str()
            .expect("temporary source path should be utf-8"),
        "FreshNoise",
    );
    assert!(refreshed.contains_key("std.demo.Refreshed"));
    assert!(!refreshed.contains_key("std.demo.Unrelated"));
    let no_partial_match = load_discovery_interfaces_for_symbol_from_file_set(
        source_path
            .to_str()
            .expect("temporary source path should be utf-8"),
        "Noise",
    );
    assert!(!no_partial_match.contains_key("std.demo.Refreshed"));

    let _ = fs::remove_dir_all(&root);
}

/// Verifies symbol-directed discovery against the checked-in package catalog.
#[test]
pub(super) fn discovery_symbol_filter_parses_only_matching_checked_in_summaries() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("std/binary/Binary.terl");
    let interfaces = load_discovery_interfaces_for_symbol_from_file_set(
        source
            .to_str()
            .expect("checked-in source path should be utf-8"),
        "protocol_shape_set_encode_exact",
    );

    assert!(interfaces.contains_key("std.binary.Binary"));
    assert_eq!(interfaces.len(), 1, "interfaces: {interfaces:?}");
    let repeated = load_discovery_interfaces_for_symbol_from_file_set(
        source
            .to_str()
            .expect("checked-in source path should be utf-8"),
        "protocol_shape_set_encode_exact",
    );
    assert!(repeated.contains_key("std.binary.Binary"));
    assert_eq!(repeated.len(), interfaces.len());
}
