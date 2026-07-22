//! Tests for implementation-free interface prepass reuse.

use std::fs;

use crate::commands::artifacts::{fingerprint, DependencyManifest};
use crate::support::test_fs;
use crate::terlan_hir::parse_interface_file;
use crate::terlan_syntax::cached_canonical_terlan_syntax_contract_identity;
use crate::CliState;

use super::source_roots::{cached_interface_is_current, prepare_source_root_interfaces};

/// Proves only a complete current interface and manifest pair skips parsing.
#[test]
fn interface_prepass_reuses_only_complete_current_signature_artifacts() {
    let root = test_fs::temp_dir("build_source_roots", "current_interface_cache");
    let source_root = root.join("src");
    let source_path = source_root.join("app/Dependency.terl");
    let cache_dir = root.join("cache");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("create source directory");
    let source = "module app.Dependency.\n\npub value(): Int -> 1.\n";
    fs::write(&source_path, source).expect("write source");
    let state = CliState {
        incremental: true,
        cache_dir: Some(cache_dir.clone()),
        ..CliState::default()
    };

    prepare_source_root_interfaces(&source_root, &state).expect("prepare initial interface");
    assert!(!cached_interface_is_current(
        &source_root,
        &source_path,
        source,
        &cache_dir
    ));
    let (_, interface) = parse_interface_file(&cache_dir.join("app.Dependency.typi"))
        .expect("parse prepared interface");
    let manifest = DependencyManifest {
        module: "app.Dependency".to_string(),
        syntax_contract_identity: cached_canonical_terlan_syntax_contract_identity()
            .expect("current syntax identity"),
        source_hash: fingerprint(source.as_bytes()),
        interface_hash: fingerprint(interface.to_terlan_interface_type_text().as_bytes()),
        interface_doc_hash: fingerprint(interface.to_terlan_interface_doc_text().as_bytes()),
        dependencies: Vec::new(),
    };
    fs::write(
        cache_dir.join("app.Dependency.typi.deps"),
        manifest.encode(),
    )
    .expect("write interface dependency manifest");
    assert!(cached_interface_is_current(
        &source_root,
        &source_path,
        source,
        &cache_dir
    ));

    fs::write(cache_dir.join("app.Dependency.typi"), "poisoned").expect("poison interface");
    assert!(!cached_interface_is_current(
        &source_root,
        &source_path,
        source,
        &cache_dir
    ));
    prepare_source_root_interfaces(&source_root, &state).expect("repair poisoned interface");
    assert!(cached_interface_is_current(
        &source_root,
        &source_path,
        source,
        &cache_dir
    ));

    let changed = "module app.Dependency.\n\npub value(): Int -> 2.\n";
    assert!(!cached_interface_is_current(
        &source_root,
        &source_path,
        changed,
        &cache_dir
    ));
    fs::remove_dir_all(root).expect("remove interface cache fixture");
}
