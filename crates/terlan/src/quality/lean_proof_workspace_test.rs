use super::*;
use crate::support::test_fs::TestDirectory;

fn fixture() -> TestDirectory {
    let root = TestDirectory::new("lean_workspace", "inputs");
    fs::create_dir_all(root.join("proofs/lean/Terlan")).unwrap();
    fs::write(root.join("proofs/lean/lakefile.lean"), "import Lake\n").unwrap();
    fs::write(
        root.join("proofs/lean/lean-toolchain"),
        "leanprover/lean4:v4.31.0\n",
    )
    .unwrap();
    fs::write(
        root.join("proofs/lean/lake-manifest.json"),
        "{\"packages\":[]}\n",
    )
    .unwrap();
    fs::write(
        root.join("proofs/lean/Terlan/Proof.lean"),
        "example : True := True.intro\n",
    )
    .unwrap();
    root
}

#[test]
fn proof_workspace_owns_copied_bytes_and_declared_manifests() {
    let root = fixture();
    fs::write(root.join("contract.toml"), "version=1\n").unwrap();
    let private = ProofWorkspace::create(&root, &["contract.toml".into()]).unwrap();
    fs::write(root.join("proofs/lean/Terlan/Proof.lean"), "changed\n").unwrap();
    assert_eq!(
        fs::read_to_string(private.root().join("proofs/lean/Terlan/Proof.lean")).unwrap(),
        "example : True := True.intro\n"
    );
    assert_eq!(
        fs::read_to_string(private.root().join("contract.toml")).unwrap(),
        "version=1\n"
    );
    let path = private.root().to_path_buf();
    private.close().unwrap();
    assert!(!path.exists());
}

#[test]
fn proof_workspace_does_not_adopt_earlier_process_residue() {
    let root = fixture();
    let parent = workspace_parent(&root).unwrap();
    let residue = parent.join("replay-earlier-process");
    fs::create_dir(&residue).unwrap();
    fs::write(residue.join("active.lean"), "live producer").unwrap();
    ProofWorkspace::create(&root, &[]).unwrap().close().unwrap();
    assert_eq!(
        fs::read_to_string(residue.join("active.lean")).unwrap(),
        "live producer"
    );
}

#[test]
fn proof_workspace_cleans_on_unwind_without_removing_another_owner() {
    let root = fixture();
    let retained = ProofWorkspace::create(&root, &[]).unwrap();
    let path = std::sync::Mutex::new(PathBuf::new());
    assert!(std::panic::catch_unwind(|| {
        let private = ProofWorkspace::create(&root, &[]).unwrap();
        *path.lock().unwrap() = private.root().to_path_buf();
        panic!("fixture unwind");
    })
    .is_err());
    assert!(!path.into_inner().unwrap().exists());
    assert!(retained.root().is_dir());
    retained.close().unwrap();
}

#[test]
fn proof_workspace_rejects_escaping_inputs_before_creating_scratch() {
    let root = fixture();
    for input in [
        "../outside",
        "/outside",
        "a/../../outside",
        "a\\outside",
        ".",
    ] {
        assert!(
            ProofWorkspace::create(&root, &[input.into()]).is_err(),
            "{input}"
        );
    }
    assert!(!root.join("target").exists());
}

#[cfg(unix)]
#[test]
fn proof_workspace_rejects_source_and_parent_symlinks() {
    use std::os::unix::fs::symlink;
    let root = fixture();
    symlink(
        root.join("proofs/lean/Terlan/Proof.lean"),
        root.join("proofs/lean/Terlan/Alias.lean"),
    )
    .unwrap();
    assert!(ProofWorkspace::create(&root, &[]).is_err());
    fs::remove_file(root.join("proofs/lean/Terlan/Alias.lean")).unwrap();
    let other = TestDirectory::new("lean_workspace", "foreign");
    symlink(other.path(), root.join("target")).unwrap();
    assert!(ProofWorkspace::create(&root, &[]).is_err());
    assert_eq!(fs::read_dir(other.path()).unwrap().count(), 0);
}

#[test]
fn proof_workspace_rejects_oversized_inputs_without_reading_them() {
    let root = fixture();
    fs::File::create(root.join("proofs/lean/Large.lean"))
        .unwrap()
        .set_len(MAX_INPUT_BYTES + 1)
        .unwrap();
    assert!(ProofWorkspace::create(&root, &[]).is_err());
    assert!(!root.join("target").exists());
}

#[cfg(unix)]
#[test]
fn proof_workspace_normalizes_absolute_output_from_relative_repository_root() {
    let root = fixture();
    let cwd = fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
    let mut relative = PathBuf::new();
    for component in cwd.components() {
        if matches!(component, Component::Normal(_)) {
            relative.push("..");
        }
    }
    relative.push(root.path().strip_prefix("/").unwrap());
    assert!(!relative.is_absolute());
    let private = ProofWorkspace::create(&relative, &[]).unwrap();
    assert!(private.root().is_absolute());
    let output = super::super::run_proof_command(
        std::process::Command::new("/bin/sh")
            .args(["-c", "pwd -P"])
            .current_dir(private.root()),
        std::time::Duration::from_secs(2),
    )
    .unwrap();
    let normalized = super::super::normalize_execution(private.root(), output);
    assert_eq!(normalized.exit, 0);
    assert_eq!(normalized.stdout, "<repo>");
    private.close().unwrap();
}
