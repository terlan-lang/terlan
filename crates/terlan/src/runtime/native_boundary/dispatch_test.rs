pub(super) use super::*;
pub(super) use crate::terlan_native_boundary::resource::{ResourceStore, ResourceValue};

#[cfg(test)]
#[path = "dispatch_test/dispatch_fixtures.rs"]
mod dispatch_fixtures;
use dispatch_fixtures::*;
#[cfg(test)]
#[path = "dispatch_test/archive_operations.rs"]
mod archive_operations;
#[cfg(test)]
#[path = "dispatch_test/toml_operations.rs"]
mod toml_operations;
#[cfg(test)]
#[path = "dispatch_test/typed_handle_operations.rs"]
mod typed_handle_operations;

#[test]
fn selected_file_hash_preserves_release_surface_framing() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "selected_hash");
    std::fs::write(root.join("a.txt"), "a").expect("write first selected file");
    std::fs::write(root.join("b.txt"), "b").expect("write second selected file");
    assert_eq!(
        dispatch(
            "std.crypto.hash.sha256_selected_files",
            &[
                NativeBoundaryValue::Text(root.to_string_lossy().into_owned()),
                NativeBoundaryValue::List(vec![
                    NativeBoundaryValue::Text("a.txt".to_string()),
                    NativeBoundaryValue::Text("b.txt".to_string()),
                ]),
            ],
        ),
        Ok(NativeBoundaryValue::Text(
            "71d40351bccb0eba3ca4fdbb0dfdf03b024d221805e140c74c5c7395fdf7c33b".to_string()
        ))
    );
}

#[test]
fn labeled_file_content_hash_preserves_compilation_input_framing() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "content_hash");
    let first = root.join("first.rs");
    let second = root.join("second.rs");
    std::fs::write(&first, "a").expect("write first compilation input");
    std::fs::write(&second, "bb").expect("write second compilation input");
    let labeled = |path: &std::path::Path, label: &str| NativeBoundaryValue::Record {
        name: "LabeledFile".to_string(),
        fields: vec![
            (
                "path".to_string(),
                NativeBoundaryValue::Text(path.to_string_lossy().into_owned()),
            ),
            (
                "label".to_string(),
                NativeBoundaryValue::Text(label.to_string()),
            ),
        ],
    };
    assert_eq!(
        dispatch(
            "std.crypto.hash.sha256_labeled_file_contents",
            &[NativeBoundaryValue::List(vec![
                labeled(&first, "a.txt"),
                labeled(&second, "b.txt"),
            ])],
        ),
        Ok(NativeBoundaryValue::Text(
            "ae6e2c0577dfe61fdefcf0cb3a332286d15a229a1f258caf3471bb9511c56c5b".to_string()
        ))
    );
}

#[test]
fn labeled_file_audit_hashes_and_scans_each_stream_across_chunk_boundaries() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "labeled_audit");
    let portable = root.join("portable.txt");
    let leaked = root.join("leaked.txt");
    std::fs::write(&portable, "portable").expect("write portable audit fixture");
    let mut leaked_bytes = vec![b'x'; 65_533];
    leaked_bytes.extend_from_slice(b"/home/alice/project");
    std::fs::write(&leaked, leaked_bytes).expect("write cross-chunk audit fixture");
    let files = NativeBoundaryValue::List(vec![
        NativeBoundaryValue::Record {
            name: "LabeledFile".to_string(),
            fields: vec![
                (
                    "path".to_string(),
                    NativeBoundaryValue::Text(portable.to_string_lossy().into_owned()),
                ),
                (
                    "label".to_string(),
                    NativeBoundaryValue::Text("fixture:portable.txt".to_string()),
                ),
            ],
        },
        NativeBoundaryValue::Record {
            name: "LabeledFile".to_string(),
            fields: vec![
                (
                    "path".to_string(),
                    NativeBoundaryValue::Text(leaked.to_string_lossy().into_owned()),
                ),
                (
                    "label".to_string(),
                    NativeBoundaryValue::Text("fixture:leaked.txt".to_string()),
                ),
            ],
        },
    ]);
    let expected_digest = dispatch(
        "std.crypto.hash.sha256_labeled_file_digests",
        std::slice::from_ref(&files),
    )
    .expect("hash labeled audit fixtures");
    let audit = dispatch(
        "std.crypto.hash.audit_labeled_files",
        &[
            files,
            NativeBoundaryValue::List(vec![NativeBoundaryValue::Text("/home/".to_string())]),
        ],
    )
    .expect("audit labeled fixtures");
    let NativeBoundaryValue::Record { name, fields } = audit else {
        panic!("expected LabeledFileAudit record");
    };
    assert_eq!(name, "LabeledFileAudit");
    assert!(fields.contains(&("file_count".to_string(), NativeBoundaryValue::Int(2))));
    assert!(fields.contains(&("portable".to_string(), NativeBoundaryValue::Bool(false))));
    assert!(fields.contains(&("digest".to_string(), expected_digest)));
}

#[test]
fn labeled_file_pattern_audit_expands_bounded_patterns_and_rejects_ambiguity() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "pattern_audit");
    std::fs::create_dir_all(root.join("artifacts/nested")).expect("create artifact tree");
    std::fs::create_dir_all(root.join("metadata")).expect("create metadata directory");
    let first = root.join("artifacts/a.terl");
    let second = root.join("artifacts/nested/b.terl");
    let ignored = root.join("artifacts/nested/ignored.txt");
    let manifest = root.join("metadata/manifest.json");
    std::fs::write(&first, "first").expect("write first pattern fixture");
    std::fs::write(&second, "second").expect("write second pattern fixture");
    std::fs::write(&ignored, "ignored").expect("write ignored pattern fixture");
    std::fs::write(&manifest, "manifest").expect("write manifest fixture");

    let pattern = |id: &str, value: &str| NativeBoundaryValue::Record {
        name: "LabeledFilePattern".to_string(),
        fields: vec![
            ("id".to_string(), NativeBoundaryValue::Text(id.to_string())),
            (
                "pattern".to_string(),
                NativeBoundaryValue::Text(value.to_string()),
            ),
        ],
    };
    let labeled = |path: &std::path::Path, label: &str| NativeBoundaryValue::Record {
        name: "LabeledFile".to_string(),
        fields: vec![
            (
                "path".to_string(),
                NativeBoundaryValue::Text(path.to_string_lossy().into_owned()),
            ),
            (
                "label".to_string(),
                NativeBoundaryValue::Text(label.to_string()),
            ),
        ],
    };
    let expected = dispatch(
        "std.crypto.hash.sha256_labeled_file_digests",
        &[NativeBoundaryValue::List(vec![
            labeled(&first, "source:artifacts/a.terl"),
            labeled(&second, "source:artifacts/nested/b.terl"),
            labeled(&manifest, "metadata:metadata/manifest.json"),
        ])],
    )
    .expect("hash explicit labeled pattern fixtures");
    let audited = dispatch(
        "std.crypto.hash.audit_labeled_file_patterns",
        &[
            NativeBoundaryValue::Text(root.to_string_lossy().into_owned()),
            NativeBoundaryValue::List(vec![
                pattern("source", "artifacts/**/*.terl"),
                pattern("metadata", "metadata/*.json"),
            ]),
            NativeBoundaryValue::List(Vec::new()),
        ],
    )
    .expect("audit bounded labeled-file patterns");
    let NativeBoundaryValue::Record { name, fields } = audited else {
        panic!("expected LabeledFileAudit record");
    };
    assert_eq!(name, "LabeledFileAudit");
    assert!(fields.contains(&("file_count".to_string(), NativeBoundaryValue::Int(3))));
    assert!(fields.contains(&("portable".to_string(), NativeBoundaryValue::Bool(true))));
    assert!(fields.contains(&("digest".to_string(), expected)));

    let duplicate = dispatch(
        "std.crypto.hash.audit_labeled_file_patterns",
        &[
            NativeBoundaryValue::Text(root.to_string_lossy().into_owned()),
            NativeBoundaryValue::List(vec![
                pattern("source", "artifacts/**/*.terl"),
                pattern("again", "artifacts/a.terl"),
            ]),
            NativeBoundaryValue::List(Vec::new()),
        ],
    );
    assert!(duplicate.is_err(), "duplicate pattern matches must fail");

    let empty = dispatch(
        "std.crypto.hash.audit_labeled_file_patterns",
        &[
            NativeBoundaryValue::Text(root.to_string_lossy().into_owned()),
            NativeBoundaryValue::List(vec![pattern("missing", "artifacts/*.missing")]),
            NativeBoundaryValue::List(Vec::new()),
        ],
    );
    assert!(empty.is_err(), "empty pattern matches must fail");
}

#[test]
fn directory_tree_usage_counts_bytes_entries_and_symlinks_without_following() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "tree_usage");
    std::fs::create_dir_all(root.join("nested")).expect("create nested directory");
    std::fs::write(root.join("first.bin"), [1_u8, 2, 3]).expect("write first file");
    std::fs::write(root.join("nested/second.bin"), [4_u8, 5]).expect("write second file");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("first.bin"), root.join("linked.bin"))
        .expect("create usage symlink");

    let value = dispatch(
        "std.io.directory.tree_usage",
        &[NativeBoundaryValue::Text(
            root.to_string_lossy().into_owned(),
        )],
    )
    .expect("measure directory tree");
    let NativeBoundaryValue::Record { name, fields } = value else {
        panic!("expected TreeUsage record");
    };
    assert_eq!(name, "TreeUsage");
    let integer = |field: &str| {
        fields
            .iter()
            .find_map(|(name, value)| {
                (name == field)
                    .then_some(value)
                    .and_then(|value| match value {
                        NativeBoundaryValue::Int(value) => Some(*value),
                        _ => None,
                    })
            })
            .unwrap_or_default()
    };
    assert_eq!(integer("logical_file_bytes"), 5);
    assert!(integer("allocated_bytes") >= 5);
    assert_eq!(integer("regular_file_count"), 2);
    assert_eq!(integer("directory_count"), 2);
    #[cfg(unix)]
    assert_eq!(integer("symbolic_link_count"), 1);
}

#[test]
fn directory_recursive_exclusions_prune_exact_basenames() {
    let root = crate::support::test_fs::temp_dir(
        "native_boundary_dispatch",
        "directory_recursive_exclusions",
    );
    let kept = root.join("kept");
    let ignored = root.join("cache");
    std::fs::create_dir_all(&kept).expect("create kept directory");
    std::fs::create_dir_all(&ignored).expect("create ignored directory");
    std::fs::write(kept.join("a.py"), "kept").expect("write kept file");
    std::fs::write(ignored.join("b.py"), "ignored").expect("write ignored file");

    let value = dispatch(
        "std.io.directory.files_recursive_excluding",
        &[
            NativeBoundaryValue::Text(root.to_string_lossy().into_owned()),
            NativeBoundaryValue::List(vec![NativeBoundaryValue::Text("cache".to_string())]),
        ],
    )
    .expect("dispatch exclusion-aware traversal");
    let NativeBoundaryValue::List(files) = value else {
        panic!("expected file list, found {value:?}");
    };
    assert_eq!(files.len(), 1);
    assert!(matches!(
        &files[0],
        NativeBoundaryValue::Text(path) if path.ends_with("/kept/a.py")
    ));
}

#[test]
fn file_size_reads_metadata_and_rejects_directories() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "file_size");
    let file = root.join("value.bin");
    std::fs::write(&file, [1_u8, 2, 3]).expect("write size fixture");
    assert_eq!(
        dispatch(
            "std.io.file.size",
            &[NativeBoundaryValue::Text(
                file.to_string_lossy().into_owned()
            )],
        ),
        Ok(NativeBoundaryValue::Int(3))
    );
    let error = dispatch(
        "std.io.file.size",
        &[NativeBoundaryValue::Text(
            root.to_string_lossy().into_owned(),
        )],
    )
    .expect_err("directory is not a file");
    assert_eq!(error.code(), "file.invalid_path");
}

#[test]
fn file_timestamps_roundtrip_without_rewriting_contents() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "file_timestamps");
    let file = root.join("value.txt");
    std::fs::write(&file, "unchanged").expect("write timestamp fixture");
    let path = file.to_string_lossy().into_owned();
    let accessed = 1_700_000_000_000_000_000_i64;
    let modified = 1_700_000_001_000_000_000_i64;
    assert_eq!(
        dispatch(
            "std.io.file.set_timestamps",
            &[
                NativeBoundaryValue::Text(path.clone()),
                NativeBoundaryValue::Int(accessed),
                NativeBoundaryValue::Int(modified),
            ],
        ),
        Ok(NativeBoundaryValue::Unit)
    );
    assert_eq!(
        dispatch("std.io.file.timestamps", &[NativeBoundaryValue::Text(path)],),
        Ok(NativeBoundaryValue::Record {
            name: "FileTimestamps".to_string(),
            fields: vec![
                (
                    "accessed_unix_ns".to_string(),
                    NativeBoundaryValue::Int(accessed),
                ),
                (
                    "modified_unix_ns".to_string(),
                    NativeBoundaryValue::Int(modified),
                ),
            ],
        })
    );
    assert_eq!(
        std::fs::read_to_string(file).expect("read fixture"),
        "unchanged"
    );
}

#[test]
fn named_directory_search_returns_relative_roots_and_prunes_matches() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "named_directories");
    std::fs::create_dir_all(root.join("alpha/target/nested/target"))
        .expect("create nested target fixture");
    std::fs::create_dir_all(root.join("target")).expect("create root target fixture");
    std::fs::create_dir_all(root.join("cache/target")).expect("create excluded target fixture");
    let values =
        directory_find_named_recursive_excluding(&root.to_string_lossy(), "target", &["cache"])
            .expect("find named directories");
    assert_eq!(
        values,
        vec![
            NativeBoundaryValue::Text("alpha/target".to_string()),
            NativeBoundaryValue::Text("target".to_string()),
        ]
    );
}

#[test]
fn directory_tree_copy_is_fresh_pruned_and_symlink_safe() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "directory_tree_copy");
    let source = root.join("source");
    let destination = root.join("destination");
    std::fs::create_dir_all(source.join("kept")).expect("create kept directory");
    std::fs::create_dir_all(source.join("cache")).expect("create excluded directory");
    std::fs::write(source.join("kept/run.sh"), "#!/bin/sh\n").expect("write kept file");
    std::fs::write(source.join("ignored.tmp"), "ignored").expect("write excluded file");
    std::fs::write(source.join("cache/ignored.txt"), "ignored")
        .expect("write excluded nested file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(
            source.join("kept/run.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .expect("mark source executable");
        std::os::unix::fs::symlink(source.join("kept/run.sh"), source.join("linked.sh"))
            .expect("create source symlink");
    }

    let value = dispatch(
        "std.io.directory.copy_tree_excluding",
        &[
            NativeBoundaryValue::Text(source.to_string_lossy().into_owned()),
            NativeBoundaryValue::Text(destination.to_string_lossy().into_owned()),
            NativeBoundaryValue::List(vec![
                NativeBoundaryValue::Text("cache".to_string()),
                NativeBoundaryValue::Text("ignored.tmp".to_string()),
            ]),
        ],
    )
    .expect("dispatch safe tree copy");

    assert_eq!(value, NativeBoundaryValue::Unit);
    assert_eq!(
        std::fs::read_to_string(destination.join("kept/run.sh")).expect("read copied file"),
        "#!/bin/sh\n"
    );
    assert!(!destination.join("cache").exists());
    assert!(!destination.join("ignored.tmp").exists());
    assert!(!destination.join("linked.sh").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_ne!(
            std::fs::metadata(destination.join("kept/run.sh"))
                .expect("copied metadata")
                .permissions()
                .mode()
                & 0o111,
            0
        );
    }
}

#[test]
fn directory_tree_copy_rejects_existing_and_nested_destinations() {
    let root = crate::support::test_fs::temp_dir(
        "native_boundary_dispatch",
        "directory_tree_copy_rejections",
    );
    let source = root.join("source");
    let existing = root.join("existing");
    std::fs::create_dir_all(&source).expect("create source");
    std::fs::create_dir_all(&existing).expect("create existing destination");
    std::fs::write(source.join("value.txt"), "value").expect("write source file");

    let existing_error = dispatch(
        "std.io.directory.copy_tree_excluding",
        &[
            NativeBoundaryValue::Text(source.to_string_lossy().into_owned()),
            NativeBoundaryValue::Text(existing.to_string_lossy().into_owned()),
            NativeBoundaryValue::List(Vec::new()),
        ],
    )
    .expect_err("existing destination must fail closed");
    assert_eq!(existing_error.code(), "directory.invalid_path");

    let nested = source.join("nested");
    let nested_error = dispatch(
        "std.io.directory.copy_tree_excluding",
        &[
            NativeBoundaryValue::Text(source.to_string_lossy().into_owned()),
            NativeBoundaryValue::Text(nested.to_string_lossy().into_owned()),
            NativeBoundaryValue::List(Vec::new()),
        ],
    )
    .expect_err("nested destination must fail closed");
    assert_eq!(nested_error.code(), "directory.invalid_path");
    assert!(!nested.exists());
}

#[cfg(unix)]
#[test]
fn directory_symbolic_link_is_typed_and_never_replaces_entries() {
    let root =
        crate::support::test_fs::temp_dir("native_boundary_dispatch", "directory_symbolic_link");
    let target = root.join("target");
    let link = root.join("link");
    std::fs::create_dir_all(&target).expect("create symbolic-link target");
    std::fs::write(target.join("value.txt"), "linked").expect("write target value");

    let value = dispatch(
        "std.io.directory.create_symbolic_link",
        &[
            NativeBoundaryValue::Text(target.to_string_lossy().into_owned()),
            NativeBoundaryValue::Text(link.to_string_lossy().into_owned()),
        ],
    )
    .expect("dispatch directory symbolic link");
    assert_eq!(value, NativeBoundaryValue::Unit);
    assert_eq!(
        std::fs::read_to_string(link.join("value.txt")).expect("read linked value"),
        "linked"
    );

    let error = dispatch(
        "std.io.directory.create_symbolic_link",
        &[
            NativeBoundaryValue::Text(target.to_string_lossy().into_owned()),
            NativeBoundaryValue::Text(link.to_string_lossy().into_owned()),
        ],
    )
    .expect_err("existing symbolic-link entry must not be replaced");
    assert_eq!(error.code(), "directory.invalid_path");
}

#[test]
fn batch_text_reads_preserve_order_and_attach_failing_path() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "batch_text_reads");
    let first = root.join("first.txt");
    let second = root.join("second.txt");
    let missing = root.join("missing.txt");
    std::fs::write(&first, "first").expect("write first file");
    std::fs::write(&second, "second").expect("write second file");

    let value = dispatch(
        "std.io.file.read_text_many",
        &[NativeBoundaryValue::List(vec![
            NativeBoundaryValue::Text(second.to_string_lossy().into_owned()),
            NativeBoundaryValue::Text(first.to_string_lossy().into_owned()),
        ])],
    )
    .expect("dispatch batch text read");
    let NativeBoundaryValue::List(files) = value else {
        panic!("expected file list, found {value:?}");
    };
    assert!(matches!(
        &files[..],
        [
            NativeBoundaryValue::Record { fields: second_fields, .. },
            NativeBoundaryValue::Record { fields: first_fields, .. }
        ] if second_fields[1].1 == NativeBoundaryValue::Text("second".to_string())
            && first_fields[1].1 == NativeBoundaryValue::Text("first".to_string())
    ));

    let error = dispatch(
        "std.io.file.read_text_many",
        &[NativeBoundaryValue::List(vec![
            NativeBoundaryValue::Text(first.to_string_lossy().into_owned()),
            NativeBoundaryValue::Text(missing.to_string_lossy().into_owned()),
        ])],
    )
    .expect_err("missing batch member must fail");
    assert_eq!(error.code(), "file.not_found");
    assert_eq!(error.path(), Some(missing.to_string_lossy().as_ref()));
}

#[test]
fn directory_text_read_is_sorted_immediate_and_omits_symlinks() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "directory_text_read");
    let nested = root.join("nested");
    std::fs::create_dir_all(&nested).expect("create nested directory");
    std::fs::write(root.join("b.txt"), "second").expect("write second file");
    std::fs::write(root.join("a.txt"), "first").expect("write first file");
    std::fs::write(nested.join("ignored.txt"), "ignored").expect("write nested file");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("a.txt"), root.join("linked.txt"))
        .expect("create file symlink");

    let value = dispatch(
        "std.io.file.read_text_directory",
        &[NativeBoundaryValue::Text(
            root.to_string_lossy().into_owned(),
        )],
    )
    .expect("dispatch directory text read");
    let NativeBoundaryValue::List(files) = value else {
        panic!("expected file list, found {value:?}");
    };
    assert_eq!(files.len(), 2);
    assert!(matches!(
        &files[..],
        [
            NativeBoundaryValue::Record { fields: first, .. },
            NativeBoundaryValue::Record { fields: second, .. }
        ] if matches!(&first[0].1, NativeBoundaryValue::Text(path) if path.ends_with("/a.txt"))
            && matches!(&second[0].1, NativeBoundaryValue::Text(path) if path.ends_with("/b.txt"))
    ));
}

#[test]
fn recursive_text_read_prunes_exact_directory_names() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "recursive_text_read");
    std::fs::create_dir_all(root.join("kept")).expect("create kept directory");
    std::fs::create_dir_all(root.join("cache")).expect("create cache directory");
    std::fs::write(root.join("kept/a.rs"), "kept").expect("write kept source");
    std::fs::write(root.join("cache/b.rs"), "ignored").expect("write ignored source");
    let value = dispatch(
        "std.io.file.read_text_tree_excluding",
        &[
            NativeBoundaryValue::Text(root.to_string_lossy().into_owned()),
            NativeBoundaryValue::List(vec![NativeBoundaryValue::Text("cache".to_string())]),
        ],
    )
    .expect("dispatch recursive text read");
    let NativeBoundaryValue::List(files) = value else {
        panic!("expected file list, found {value:?}");
    };
    assert!(matches!(
        &files[..],
        [NativeBoundaryValue::Record { fields, .. }]
            if matches!(&fields[0].1, NativeBoundaryValue::Text(path) if path.ends_with("/kept/a.rs"))
    ));
}

#[test]
fn recursive_text_read_filters_suffixes_before_decoding() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "matching_text_read");
    std::fs::write(root.join("source.terl"), "module source.").expect("write source");
    std::fs::write(root.join("binary.dat"), [0xff, 0xfe]).expect("write non-UTF-8 file");
    std::fs::write(root.join("ignoredTest.terl"), [0xff, 0xfe])
        .expect("write excluded non-UTF-8 source");
    let value = dispatch(
        "std.io.file.read_text_tree_matching",
        &[
            NativeBoundaryValue::Text(root.to_string_lossy().into_owned()),
            NativeBoundaryValue::List(Vec::new()),
            NativeBoundaryValue::List(vec![NativeBoundaryValue::Text(".terl".to_string())]),
            NativeBoundaryValue::List(vec![NativeBoundaryValue::Text("Test.terl".to_string())]),
            NativeBoundaryValue::Int(0),
            NativeBoundaryValue::Int(16),
        ],
    )
    .expect("dispatch suffix-filtered recursive text read");
    let NativeBoundaryValue::List(files) = value else {
        panic!("expected file list, found {value:?}");
    };
    assert!(matches!(
        &files[..],
        [NativeBoundaryValue::Record { fields, .. }]
            if matches!(&fields[0].1, NativeBoundaryValue::Text(path) if path.ends_with("/source.terl"))
    ));

    std::fs::write(root.join("later.terl"), "module later.").expect("write later source");
    let page = dispatch(
        "std.io.file.read_text_tree_matching",
        &[
            NativeBoundaryValue::Text(root.to_string_lossy().into_owned()),
            NativeBoundaryValue::List(Vec::new()),
            NativeBoundaryValue::List(vec![NativeBoundaryValue::Text(".terl".to_string())]),
            NativeBoundaryValue::List(vec![NativeBoundaryValue::Text("Test.terl".to_string())]),
            NativeBoundaryValue::Int(1),
            NativeBoundaryValue::Int(1),
        ],
    )
    .expect("dispatch second suffix-filtered page");
    assert!(matches!(
        page,
        NativeBoundaryValue::List(files)
            if matches!(&files[..], [NativeBoundaryValue::Record { fields, .. }]
                if matches!(&fields[0].1, NativeBoundaryValue::Text(path) if path.ends_with("/source.terl")))
    ));
}
