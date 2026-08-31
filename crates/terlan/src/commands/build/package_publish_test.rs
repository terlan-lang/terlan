use super::*;

#[test]
fn sealing_is_deterministic_and_excludes_workspace_outputs() {
    let root = temp("source");
    fs::create_dir_all(root.join("src/demo")).unwrap();
    fs::create_dir_all(root.join("_build")).unwrap();
    fs::write(root.join("terlan.toml"), "[package]\nname = \"demo\"\nversion = \"1.0.0\"\ndescription = \"A deterministic package fixture.\"\nlicense = \"Apache-2.0 OR MIT\"\nrepository = \"https://github.com/terlan-lang/terlan\"\nlinks = [\"https://terlan.org/packages/demo\"]\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n").unwrap();
    fs::write(root.join("src/demo/Main.terl"), "module demo.Main.\n").unwrap();
    fs::write(root.join("_build/secret.txt"), "must-not-ship").unwrap();
    let first = temp("first");
    let second = temp("second");
    let one = seal_publish_dry_run(&root, &first).unwrap();
    let two = seal_publish_dry_run(&root, &second).unwrap();
    assert_eq!(one.archive_sha256, two.archive_sha256);
    assert_eq!(
        fs::read(one.request).unwrap(),
        fs::read(two.request).unwrap()
    );
    let request: PublishRequest = serde_json::from_slice(
        &fs::read(first.join("package/demo-1.0.0.publish-request.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(request.package_version.archive.file_count, 2);
    assert_eq!(
        request.package_version.repository_url,
        "https://github.com/terlan-lang/terlan"
    );
    assert_eq!(request.package_version.license, "Apache-2.0 OR MIT");
    assert_eq!(request.package_version.links[0].name, "terlan.org");
    assert_eq!(request.package_version.built_with, "terlan-0.0.8");
    assert_eq!(request.package_version.requires_terlan, ">=0.0.8, <0.1.0");
    assert_eq!(
        request.package_version.source_identity.kind,
        SourceIdentityKind::ArtifactSet
    );
    assert_eq!(
        request.package_version.source_identity.verification,
        SourceIdentityVerification::RegistryDerived
    );
    assert_eq!(
        request.package_version.source_identity.value,
        request.package_version.provenance.value
    );
    assert!(request.package_version.documentation.is_none());
    assert!(request.documentation_upload.is_none());
    assert!(request
        .package_version
        .artifacts
        .iter()
        .all(|artifact| !artifact.path.contains("secret")));
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(first).unwrap();
    fs::remove_dir_all(second).unwrap();
}

#[test]
fn sealing_emits_an_independent_documentation_archive() {
    let root = temp("documentation-source");
    fs::create_dir_all(root.join("src/demo")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("terlan.toml"), "[package]\nname = \"demo\"\nversion = \"1.0.0\"\ndescription = \"A documentation archive fixture.\"\nlicense = \"MIT\"\nrepository = \"https://github.com/terlan-lang/terlan\"\nlinks = [\"https://terlan.org/packages/demo\"]\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n").unwrap();
    fs::write(root.join("src/demo/Main.terl"), "module demo.Main.\n").unwrap();
    fs::write(
        root.join("docs/README.md"),
        "# Demo\n\nSafe documentation.\n",
    )
    .unwrap();
    let output = temp("documentation-output");

    let sealed = seal_publish_dry_run(&root, &output).unwrap();
    let documentation = sealed
        .documentation
        .as_ref()
        .expect("documentation archive should be emitted");
    let request: PublishRequest =
        serde_json::from_slice(&fs::read(&sealed.request).unwrap()).unwrap();
    let identity = request
        .package_version
        .documentation
        .expect("documentation identity should be signed");

    assert_ne!(
        fs::read(&sealed.archive).unwrap(),
        fs::read(documentation).unwrap()
    );
    assert_eq!(identity.digest.value, hash_file(documentation).unwrap());
    assert_eq!(identity.file_count, 1);
    assert_eq!(
        request.documentation_upload.as_deref(),
        documentation.file_name().and_then(|name| name.to_str())
    );

    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(output).unwrap();
}

#[test]
fn sealing_requires_a_public_https_repository() {
    let root = temp("repository");
    fs::create_dir_all(root.join("src/demo")).unwrap();
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"demo\"\nversion = \"1.0.0\"\nrepository = \"http://example.com/private\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n",
    )
    .unwrap();
    fs::write(root.join("src/demo/Main.terl"), "module demo.Main.\n").unwrap();
    let output = temp("repository-out");
    let error = seal_publish_dry_run(&root, &output).unwrap_err();
    assert!(error.contains("[package].repository must be a valid public HTTPS URL"));
    fs::remove_dir_all(root).unwrap();
    let _ = fs::remove_dir_all(output);
}

#[test]
fn sealing_rejects_likely_secrets_without_echoing_them() {
    let root = temp("secret-scan");
    fs::create_dir_all(root.join("src/demo")).unwrap();
    fs::write(root.join("terlan.toml"), "[package]\nname = \"demo\"\nversion = \"1.0.0\"\ndescription = \"A secret scan fixture.\"\nlicense = \"MIT\"\nrepository = \"https://github.com/terlan-lang/terlan\"\nlinks = [\"https://terlan.org/packages/demo\"]\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n").unwrap();
    let leaked = "ghp_abcdefghijklmnopqrstuvwxyzABCDEFGHIJ";
    fs::write(
        root.join("src/demo/Main.terl"),
        format!("module demo.Main.\n// {leaked}\n"),
    )
    .unwrap();
    let output = temp("secret-scan-out");
    let error = seal_publish_dry_run(&root, &output).unwrap_err();
    assert!(error.contains("error[registry_secret_scan]"));
    assert!(error.contains("possible github-token credential"));
    assert!(error.contains("src/demo/Main.terl:2"));
    assert!(!error.contains(leaked));
    assert!(!output.join("package/demo-1.0.0.tar.zst").exists());
    fs::remove_dir_all(root).unwrap();
    let _ = fs::remove_dir_all(output);
}

#[cfg(unix)]
#[test]
fn sealing_rejects_source_symlinks() {
    use std::os::unix::fs::symlink;
    let root = temp("symlink");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("terlan.toml"), "[package]\nname = \"demo\"\nversion = \"1.0.0\"\nrepository = \"https://github.com/terlan-lang/terlan\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n").unwrap();
    symlink(root.join("terlan.toml"), root.join("src/link.terl")).unwrap();
    let error = seal_publish_dry_run(&root, &temp("symlink-out")).unwrap_err();
    assert!(error.contains("symlink is forbidden"));
    fs::remove_dir_all(root).unwrap();
}

fn temp(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan-package-seal-{label}-{}-{nonce}",
        std::process::id()
    ))
}
