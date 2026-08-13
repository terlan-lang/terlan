use super::*;

#[test]
fn archive_creation_is_deterministic_and_roundtrips_files() {
    for suffix in ["tar.gz", "zip"] {
        let root = crate::support::test_fs::temp_dir(
            "native_boundary_dispatch",
            &format!("archive_create_{}", suffix.replace('.', "_")),
        );
        let source = root.join("source");
        std::fs::create_dir_all(source.join("nested")).expect("create archive source");
        std::fs::write(source.join("README.txt"), "release fixture").expect("write archive text");
        let executable = source.join("nested/terlc");
        std::fs::write(&executable, "binary fixture").expect("write executable fixture");
        set_executable(&executable);

        let first = root.join(format!("first.{suffix}"));
        let second = root.join(format!("second.{suffix}"));
        create_archive(&source, &first);
        create_archive(&source, &second);
        assert_eq!(
            std::fs::read(&first).expect("read first archive"),
            std::fs::read(&second).expect("read second archive"),
            "{suffix} output must be byte-for-byte deterministic"
        );

        let extracted = root.join("extracted");
        assert_eq!(
            dispatch(
                "std.io.archive.extract",
                &[
                    NativeBoundaryValue::Text(first.to_string_lossy().into_owned()),
                    NativeBoundaryValue::Text(extracted.to_string_lossy().into_owned()),
                ],
            ),
            Ok(NativeBoundaryValue::Unit)
        );
        assert_eq!(
            std::fs::read_to_string(extracted.join("README.txt")).expect("read extracted text"),
            "release fixture"
        );
        assert_eq!(
            std::fs::read_to_string(extracted.join("nested/terlc"))
                .expect("read extracted executable"),
            "binary fixture"
        );
        #[cfg(unix)]
        assert!(is_executable(&extracted.join("nested/terlc")));
    }
}

#[test]
fn archive_creation_rejects_links_and_removes_partial_output() {
    let root = crate::support::test_fs::temp_dir("native_boundary_dispatch", "archive_create_link");
    let source = root.join("source");
    std::fs::create_dir_all(&source).expect("create archive source");
    std::fs::write(source.join("value.txt"), "value").expect("write source file");
    #[cfg(unix)]
    std::os::unix::fs::symlink(source.join("value.txt"), source.join("linked.txt"))
        .expect("create source link");
    let archive = root.join("unsafe.tar.gz");
    let result = dispatch(
        "std.io.archive.create",
        &[
            NativeBoundaryValue::Text(source.to_string_lossy().into_owned()),
            NativeBoundaryValue::Text(archive.to_string_lossy().into_owned()),
        ],
    );
    #[cfg(unix)]
    {
        let error = result.expect_err("links must be rejected");
        assert_eq!(error.code(), "archive.unsafe_entry");
        assert!(!archive.exists());
    }
    #[cfg(not(unix))]
    assert_eq!(result, Ok(NativeBoundaryValue::Unit));
}

#[cfg(unix)]
#[test]
fn executable_file_operations_preserve_contents_and_other_mode_bits() {
    use std::os::unix::fs::PermissionsExt;

    let root =
        crate::support::test_fs::temp_dir("native_boundary_dispatch", "file_executable_mode");
    let path = root.join("tool");
    std::fs::write(&path, "unchanged").expect("write executable fixture");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
        .expect("set initial permissions");
    let argument = NativeBoundaryValue::Text(path.to_string_lossy().into_owned());

    assert_eq!(
        dispatch(
            "std.io.file.set_executable",
            &[argument.clone(), NativeBoundaryValue::Bool(true)],
        ),
        Ok(NativeBoundaryValue::Unit)
    );
    assert_eq!(
        dispatch("std.io.file.is_executable", std::slice::from_ref(&argument)),
        Ok(NativeBoundaryValue::Bool(true))
    );
    assert_eq!(
        std::fs::metadata(&path)
            .expect("read enabled permissions")
            .permissions()
            .mode()
            & 0o777,
        0o751
    );
    assert_eq!(
        dispatch(
            "std.io.file.set_executable",
            &[argument.clone(), NativeBoundaryValue::Bool(false)],
        ),
        Ok(NativeBoundaryValue::Unit)
    );
    assert_eq!(
        dispatch("std.io.file.is_executable", &[argument]),
        Ok(NativeBoundaryValue::Bool(false))
    );
    assert_eq!(
        std::fs::metadata(&path)
            .expect("read disabled permissions")
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(
        std::fs::read_to_string(path).expect("read fixture"),
        "unchanged"
    );
}

fn create_archive(source: &std::path::Path, archive: &std::path::Path) {
    assert_eq!(
        dispatch(
            "std.io.archive.create",
            &[
                NativeBoundaryValue::Text(source.to_string_lossy().into_owned()),
                NativeBoundaryValue::Text(archive.to_string_lossy().into_owned()),
            ],
        ),
        Ok(NativeBoundaryValue::Unit)
    );
}

#[cfg(unix)]
fn set_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .expect("set executable fixture mode");
}

#[cfg(not(unix))]
fn set_executable(_path: &std::path::Path) {}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .expect("read executable metadata")
        .permissions()
        .mode()
        & 0o111
        != 0
}
