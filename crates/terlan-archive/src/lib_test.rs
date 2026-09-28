use super::*;

#[test]
fn zip_roundtrip_preserves_unicode_paths_and_deterministic_bytes() {
    let root = temp("zip-roundtrip");
    let source = root.join("source");
    fs::create_dir_all(source.join("empty")).unwrap();
    fs::write(source.join("résumé.txt"), b"archive payload").unwrap();
    let first = root.join("first.zip");
    let second = root.join("second.zip");
    create(&source.to_string_lossy(), &first.to_string_lossy()).unwrap();
    create(&source.to_string_lossy(), &second.to_string_lossy()).unwrap();
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    let destination = root.join("extracted");
    extract(&first.to_string_lossy(), &destination.to_string_lossy()).unwrap();
    assert_eq!(
        fs::read(destination.join("résumé.txt")).unwrap(),
        b"archive payload"
    );
    assert!(destination.join("empty").is_dir());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn zip_extraction_rejects_parent_traversal() {
    let root = temp("zip-traversal");
    fs::create_dir_all(&root).unwrap();
    let archive = root.join("attack.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
    writer
        .start_file("../escaped", SimpleFileOptions::default())
        .unwrap();
    writer.finish().unwrap();
    let destination = root.join("extracted");
    let failure = extract(&archive.to_string_lossy(), &destination.to_string_lossy()).unwrap_err();
    assert_eq!(failure.code(), "archive.unsafe_entry");
    assert!(!root.join("escaped").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tar_zstd_extraction_rejects_symlinks_and_removes_partial_output() {
    let root = temp("symlink");
    fs::create_dir_all(&root).unwrap();
    let archive = root.join("attack.tar.zst");
    let output = fs::File::create(&archive).unwrap();
    let encoder = zstd::stream::write::Encoder::new(output, 3).unwrap();
    let mut tar = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    header.set_mode(0o777);
    header.set_mtime(0);
    header.set_link_name("target").unwrap();
    header.set_cksum();
    tar.append_data(&mut header, "link", io::empty()).unwrap();
    let encoder = tar.into_inner().unwrap();
    encoder.finish().unwrap();
    let destination = root.join("output");

    let failure = extract_tar_zstd(&archive, &destination).unwrap_err();
    assert_eq!(failure.code(), "archive.unsafe_entry");
    assert!(!destination.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tar_zstd_extraction_rejects_unpacked_and_entry_count_bombs() {
    let root = temp("bomb");
    fs::create_dir_all(&root).unwrap();
    let archive = root.join("bomb.tar.zst");
    write_regular_archive(
        &archive,
        &[("one", b"12345678901".as_slice()), ("two", b"2".as_slice())],
    );

    let byte_destination = root.join("byte-output");
    fs::create_dir(&byte_destination).unwrap();
    let byte_failure = extract_tar_zstd_bounded(
        &archive,
        &byte_destination,
        &archive.to_string_lossy(),
        &byte_destination.to_string_lossy(),
        10,
        4_096,
        240,
    )
    .unwrap_err();
    assert_eq!(byte_failure.code(), "archive.limit");

    let count_destination = root.join("count-output");
    fs::create_dir(&count_destination).unwrap();
    let count_failure = extract_tar_zstd_bounded(
        &archive,
        &count_destination,
        &archive.to_string_lossy(),
        &count_destination.to_string_lossy(),
        256 * 1024 * 1024,
        1,
        240,
    )
    .unwrap_err();
    assert_eq!(count_failure.code(), "archive.limit");
    fs::remove_dir_all(root).unwrap();
}

fn write_regular_archive(path: &Path, files: &[(&str, &[u8])]) {
    let output = fs::File::create(path).unwrap();
    let encoder = zstd::stream::write::Encoder::new(output, 3).unwrap();
    let mut tar = tar::Builder::new(encoder);
    for (name, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_cksum();
        tar.append_data(&mut header, name, *bytes).unwrap();
    }
    let encoder = tar.into_inner().unwrap();
    encoder.finish().unwrap();
}

fn temp(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan-archive-adversarial-{label}-{}-{nonce}",
        std::process::id()
    ))
}
