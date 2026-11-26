//! Tests for implementation-free interface prepass reuse.

use std::{fs, path::Path};

use crate::commands::artifacts::{fingerprint, DependencyManifest};
use crate::support::test_fs;
use crate::terlan_hir::parse_interface_file;
use crate::terlan_syntax::cached_canonical_terlan_syntax_contract_identity;
use crate::CliState;

use super::source_roots::{cached_interface_is_current, prepare_source_root_interfaces};
use super::write_build_file;

/// A contract exposes fewer declarations than its implementation, including on
/// repeat prepasses using a populated compiler cache.
#[test]
fn interface_first_build_prepass_publishes_only_declared_public_api() {
    let directory = test_fs::TestDirectory::new("interfaces", "build_prepass_contract");
    let root = directory.path().join("src");
    fs::create_dir_all(root.join("app")).unwrap();
    fs::write(
        root.join("app/Api.terl"),
        "module app.Api. pub value(): Int -> 1. pub helper(): Int -> 2.",
    )
    .unwrap();
    fs::write(
        root.join("app/Api.terli"),
        "module app.Api. pub value(): Int.",
    )
    .unwrap();
    let state = CliState {
        cache_dir: Some(directory.path().join("cache")),
        incremental: true,
        ..CliState::default()
    };
    for _ in 0..2 {
        prepare_source_root_interfaces(&root, &state).unwrap();
        let (_, interface) =
            parse_interface_file(&state.cache_dir.as_ref().unwrap().join("app.Api.typi")).unwrap();
        assert!(interface.functions.contains_key(&("value".to_string(), 0)));
        assert!(!interface.functions.contains_key(&("helper".to_string(), 0)));
    }
}

#[test]
fn interface_first_build_prepass_validates_external_contracts_and_preserves_source_precedence() {
    let directory = test_fs::TestDirectory::new("interfaces", "build_external_contracts");
    let root = directory.path().join("src");
    let cache = directory.path().join("cache");
    fs::create_dir_all(root.join("api")).unwrap();
    fs::create_dir_all(&cache).unwrap();
    fs::write(
        root.join("api/Value.terli"),
        "module api.Value. import dep.{extra}. pub value(): Int.",
    )
    .unwrap();
    fs::write(
        root.join("api/Client.terli"),
        "module api.Client. import api.Value.{value}. pub result(): Int.",
    )
    .unwrap();
    fs::write(
        cache.join("api.Value.terli"),
        "module api.Value. pub value(): String. pub invalid(): Missing.",
    )
    .unwrap();
    fs::write(
        cache.join("dep.terli"),
        "module dep. pub extra(): Int. pub invalid(): Missing.",
    )
    .unwrap();
    let state = CliState {
        cache_dir: Some(cache.clone()),
        ..CliState::default()
    };
    assert!(prepare_source_root_interfaces(&root, &state).is_err());
    assert!(!cache.join("api.Client.typi").exists() && !cache.join("api.Value.typi").exists());
    fs::write(cache.join("dep.terli"), "module dep. pub extra(): Int.").unwrap();
    prepare_source_root_interfaces(&root, &state).unwrap();
    let (_, interface) = parse_interface_file(&cache.join("api.Value.typi")).unwrap();
    assert_eq!(interface.functions[&("value".into(), 0)].return_type, "Int");
}

#[test]
fn interface_first_build_prepass_expands_inheritance_across_adjacent_dependency_directories() {
    let directory = test_fs::TestDirectory::new("interfaces", "build_adjacent_inheritance");
    let root = directory.path().join("src");
    let cache = directory.path().join("cache");
    fs::create_dir_all(root.join("client")).unwrap();
    fs::create_dir_all(root.join("provider")).unwrap();
    fs::write(
        root.join("provider/dep.typi"),
        "module dep. pub struct Base { code: Int }.",
    )
    .unwrap();
    fs::write(root.join("provider/Api.terli"), "module provider.Api. import dep.{Base}. pub struct Child includes Base { message: String }.").unwrap();
    fs::write(root.join("client/Client.terli"), "module client.Client. import provider.Api.{Child}. pub struct View includes Child { flag: Bool }.").unwrap();
    let state = CliState {
        cache_dir: Some(cache.clone()),
        ..CliState::default()
    };
    prepare_source_root_interfaces(&root, &state).unwrap();
    let (_, interface) = parse_interface_file(&cache.join("client.Client.typi")).unwrap();
    assert_eq!(
        interface.struct_fields["View"]
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["code", "message", "flag"]
    );
}

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

/// A failed graph preparation never publishes interfaces parsed earlier in the batch.
#[test]
fn interface_prepass_failure_preserves_the_complete_previous_generation() {
    let directory = test_fs::TestDirectory::new("interfaces", "transactional_prepass");
    let root = directory.path().join("src");
    let cache = directory.path().join("cache");
    fs::create_dir_all(root.join("app")).expect("source layout");
    let stable = root.join("app/Alpha.terl");
    let final_module = root.join("app/Zulu.terl");
    fs::write(&stable, "module app.Alpha.\npub value(): Int -> 1.\n").expect("stable source");
    fs::write(&final_module, "module app.Zulu.\npub value(): Int -> 2.\n").expect("final source");
    let state = CliState {
        cache_dir: Some(cache.clone()),
        incremental: true,
        ..CliState::default()
    };
    prepare_source_root_interfaces(&root, &state).expect("initial interface generation");
    let alpha_interface = fs::read(cache.join("app.Alpha.typi")).expect("alpha interface");
    let alpha_hash = fs::read(cache.join("app.Alpha.typi.hash")).expect("alpha hash");
    let zulu_interface = fs::read(cache.join("app.Zulu.typi")).expect("zulu interface");
    let zulu_hash = fs::read(cache.join("app.Zulu.typi.hash")).expect("zulu hash");

    fs::write(
        &stable,
        "module app.Alpha.\npub value(): String -> \"changed\".\n",
    )
    .expect("changed source");
    fs::write(
        root.join("app/Middle.terl"),
        "module app.Middle.\npub value(): Int -> 3.\n",
    )
    .expect("new source");
    fs::write(&final_module, "module app.Zulu.\npub broken(: Int -> 2.\n").expect("invalid source");

    assert!(prepare_source_root_interfaces(&root, &state).is_err());
    assert_eq!(
        fs::read(cache.join("app.Alpha.typi")).expect("preserved alpha interface"),
        alpha_interface
    );
    assert_eq!(
        fs::read(cache.join("app.Alpha.typi.hash")).expect("preserved alpha hash"),
        alpha_hash
    );
    assert_eq!(
        fs::read(cache.join("app.Zulu.typi")).expect("preserved zulu interface"),
        zulu_interface
    );
    assert_eq!(
        fs::read(cache.join("app.Zulu.typi.hash")).expect("preserved zulu hash"),
        zulu_hash
    );
    assert!(!cache.join("app.Middle.typi").exists());
    assert!(!cache.join("app.Middle.typi.hash").exists());
}

/// Build cache replacement is complete and never leaves publication scratch.
#[test]
fn build_output_publication_is_atomic_and_incremental() {
    let root = tempfile::tempdir().expect("atomic output root");
    let target = root.path().join("module.typi");
    fs::write(&target, b"previous interface").expect("seed interface");

    write_build_file(&target, b"complete replacement", false).expect("replace interface");
    write_build_file(&target, b"complete replacement", true).expect("reuse interface");

    assert_eq!(
        fs::read(&target).expect("read interface"),
        b"complete replacement"
    );
    assert_eq!(fs::read_dir(root.path()).expect("read root").count(), 1);
}

/// Failed or malformed publications preserve destinations and clean scratch.
#[test]
fn build_output_publication_failures_leave_no_partial_artifact() {
    let root = tempfile::tempdir().expect("failed output root");
    let target = root.path().join("module.typi");
    fs::create_dir(&target).expect("destination blocker");

    assert!(write_build_file(&target, b"partial interface", false).is_err());
    assert!(target.is_dir());
    assert_eq!(fs::read_dir(root.path()).expect("read root").count(), 1);

    let missing_parent = root.path().join("missing/module.typi");
    assert!(write_build_file(&missing_parent, b"interface", false).is_err());
    assert!(write_build_file(Path::new(""), b"interface", false).is_err());
}
