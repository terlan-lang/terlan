use super::{request_path, response};
use std::cell::Cell;

#[test]
fn static_file_preserves_binary_storage_and_server_headers() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("payload.bin");
    std::fs::write(&path, [0, 255, 13, 10]).unwrap();
    let allocation = Cell::new(std::ptr::null());
    let read = |path: &std::path::Path| {
        response(path, 200, None, false, |content_type, bytes| {
            assert_eq!(content_type, "application/octet-stream");
            allocation.set(bytes.as_ptr());
            bytes
        })
        .unwrap()
    };
    assert_eq!(read(&root.path().join("missing")).status(), 404);
    assert!(allocation.get().is_null());
    let output = read(&path);
    assert_eq!(output.status(), 200);
    assert_eq!(output.body().as_ref(), [0, 255, 13, 10]);
    assert_eq!(output.body().as_ptr(), allocation.get());
    assert_eq!(output.headers()["content-length"], "4");
    assert_eq!(output.headers()["cache-control"], "no-cache");
    assert_eq!(output.headers()["x-content-type-options"], "nosniff");
    assert!(!output.headers().contains_key("connection"));
}

#[test]
fn file_metadata_and_head_use_the_transformed_representation() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("index.html");
    std::fs::write(&path, b"html").unwrap();
    for head in [false, true] {
        let calls = Cell::new(0);
        let output = response(
            &path,
            202,
            Some("application/custom"),
            head,
            |mime, mut bytes| {
                assert_eq!(mime, "application/custom");
                calls.set(calls.get() + 1);
                bytes.extend_from_slice(b"-transformed");
                bytes
            },
        )
        .unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(output.status(), 202);
        assert_eq!(output.headers()["content-type"], "application/custom");
        assert_eq!(output.headers()["content-length"], "16");
        assert_eq!(
            output.body().as_ref(),
            if head { &b""[..] } else { b"html-transformed" }
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"html");
    }
}

#[test]
fn failed_reads_are_not_transformed_and_never_expose_host_paths() {
    let root = tempfile::tempdir().unwrap();
    for path in [root.path().join("missing.html"), root.path().to_path_buf()] {
        for head in [false, true] {
            let transformed = Cell::new(false);
            let output = response(&path, 206, Some("text/html"), head, |_, bytes| {
                transformed.set(true);
                bytes
            })
            .unwrap();
            assert!(
                !transformed.get(),
                "failed reads must not invoke a host transformation"
            );
            assert_eq!(output.status(), 404);
            assert_eq!(
                output.headers()["content-type"],
                "text/plain; charset=utf-8"
            );
            assert_eq!(output.headers()["content-length"], "9");
            assert_eq!(
                output.body().as_ref(),
                if head { &b""[..] } else { b"not found" }
            );
        }
    }
}

#[test]
fn invalid_metadata_is_rejected_by_the_maintained_response_builder() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("body.txt");
    std::fs::write(&path, "body").unwrap();
    for (status, mime, code) in [
        (99, "text/plain", "http.response.invalid_status"),
        (
            200,
            "text/plain\r\nx-injected: yes",
            "http.response.invalid_content_type",
        ),
    ] {
        let error = response(&path, status, Some(mime), false, |_, bytes| bytes).unwrap_err();
        assert_eq!(error.code(), code);
    }
}

#[test]
fn asset_resolution_selects_files_and_directory_indexes() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("index.html"), "index").unwrap();
    std::fs::write(root.path().join("token"), "extensionless").unwrap();
    std::fs::write(root.path().join("body.txt"), "text").unwrap();
    for directory in ["docs", "dir.with.extension"] {
        std::fs::create_dir(root.path().join(directory)).unwrap();
        std::fs::write(root.path().join(directory).join("index.html"), "index").unwrap();
    }
    for (request, expected) in [
        ("/", "index.html"),
        ("///", "index.html"),
        ("/token", "token"),
        ("/body.txt", "body.txt"),
        ("/docs", "docs/index.html"),
        ("/docs/", "docs/index.html"),
        ("/dir.with.extension", "dir.with.extension/index.html"),
    ] {
        assert_eq!(
            request_path(root.path(), request),
            Some(root.path().join(expected).canonicalize().unwrap())
        );
    }
    for (request, expected) in [
        ("/missing", "missing/index.html"),
        ("/missing.txt", "missing.txt"),
    ] {
        assert_eq!(
            request_path(root.path(), request),
            Some(root.path().join(expected))
        );
    }
    for request in ["/../secret", "/docs/../../secret", "/a\\b", "/a\0b"] {
        assert!(request_path(root.path(), request).is_none(), "{request:?}");
    }
    assert!(request_path(&root.path().join("missing"), "/").is_none());
}

#[cfg(unix)]
#[test]
fn final_index_symlinks_cannot_escape_the_package() {
    use std::os::unix::fs::symlink;
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("package");
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(parent.path().join("secret.html"), "secret").unwrap();
    for index in [root.join("index.html"), root.join("docs/index.html")] {
        symlink(parent.path().join("secret.html"), index).unwrap();
    }
    for request in ["/", "/index.html", "/docs", "/docs/", "/docs/index.html"] {
        assert!(request_path(&root, request).is_none(), "{request}");
    }
    std::fs::remove_file(root.join("index.html")).unwrap();
    std::fs::write(root.join("actual.html"), "inside").unwrap();
    symlink("actual.html", root.join("index.html")).unwrap();
    assert_eq!(request_path(&root, "/"), Some(root.join("actual.html")));
    symlink("docs", root.join("alias")).unwrap();
    assert!(request_path(&root, "/alias").is_none());
    symlink(&root, parent.path().join("linked-root")).unwrap();
    assert_eq!(
        request_path(&parent.path().join("linked-root"), "/"),
        Some(root.join("actual.html"))
    );
}
