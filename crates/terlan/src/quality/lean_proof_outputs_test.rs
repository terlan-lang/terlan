use super::*;
use std::fs;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-proof-outputs-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("target/quality/preparation/candidate/proof.work")).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }

    fn workspace(&self) -> PathBuf {
        self.0
            .join("target/quality/preparation/candidate/proof.work")
    }

    fn paths(&self) -> [PathBuf; 3] {
        ["replay.json", "track.json", "baseline.tsv"].map(|name| self.workspace().join(name))
    }

    fn capture(&self, paths: &[PathBuf; 3]) -> QualityResult<ReportPaths> {
        ReportPaths::from_lookup(&self.0, |name| {
            VARIABLES
                .iter()
                .position(|key| *key == name)
                .map(|index| paths[index].clone().into_os_string())
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn lean_proof_outputs_single_report_reuses_private_path_contract() {
    let fixture = Fixture::new();
    let default = "target/quality/report.json";
    assert_eq!(
        single_from_value(&fixture.0, default, None).unwrap(),
        fixture.0.join(default)
    );
    let output = fixture.workspace().join("policy.json");
    assert_eq!(
        single_from_value(&fixture.0, default, Some(output.clone().into())).unwrap(),
        output
    );
    for path in [
        PathBuf::new(),
        PathBuf::from("relative.json"),
        fixture.0.join(default),
        fixture.workspace().join("../escape.json"),
    ] {
        assert!(single_from_value(&fixture.0, default, Some(path.into())).is_err());
    }
    fs::write(&output, "retained").unwrap();
    assert!(single_from_value(&fixture.0, default, Some(output.clone().into())).is_err());
    assert_eq!(fs::read_to_string(output).unwrap(), "retained");
}

#[test]
fn lean_proof_outputs_default_and_complete_private_routing() {
    let fixture = Fixture::new();
    let ordinary = ReportPaths::from_lookup(&fixture.0, |_| None).unwrap();
    assert_eq!(
        ordinary.track,
        fixture
            .0
            .join("target/quality/proof-artifacts/lean-proof-track.json")
    );
    let paths = fixture.paths();
    let owned = fixture.capture(&paths).unwrap();
    assert_eq!([owned.replay, owned.track, owned.baseline], paths);
    assert_eq!(fs::read_dir(fixture.workspace()).unwrap().count(), 0);
}

#[test]
fn lean_proof_outputs_reject_partial_or_empty_staging() {
    let fixture = Fixture::new();
    let paths = fixture.paths();
    for mask in 1..7 {
        let result = ReportPaths::from_lookup(&fixture.0, |name| {
            let index = VARIABLES.iter().position(|key| *key == name).unwrap();
            (mask & (1 << index) != 0).then(|| paths[index].clone().into_os_string())
        });
        assert!(result.is_err(), "mask {mask}");
    }
    assert!(fixture
        .capture(&[PathBuf::new(), PathBuf::new(), PathBuf::new()])
        .is_err());
}

#[test]
fn lean_proof_outputs_reject_aliases_final_paths_and_existing_files() {
    let fixture = Fixture::new();
    let mut paths = fixture.paths();
    paths[1] = paths[0].clone();
    assert!(fixture.capture(&paths).is_err());
    paths = fixture.paths();
    paths[1] = fixture.0.join("target/quality/final.json");
    assert!(fixture.capture(&paths).is_err());
    paths = fixture.paths();
    paths[1] = fixture.workspace().join("../escape.json");
    assert!(fixture.capture(&paths).is_err());
    paths = fixture.paths();
    fs::write(&paths[1], "retained").unwrap();
    assert!(fixture.capture(&paths).is_err());
    assert_eq!(fs::read_to_string(&paths[1]).unwrap(), "retained");
}

#[cfg(unix)]
#[test]
fn lean_proof_outputs_reject_symlinked_workspace_and_destination() {
    let fixture = Fixture::new();
    let alias = fixture.workspace().with_file_name("alias.work");
    std::os::unix::fs::symlink(fixture.workspace(), &alias).unwrap();
    assert!(fixture
        .capture(&[alias.join("one"), alias.join("two"), alias.join("three")])
        .is_err());
    let paths = fixture.paths();
    std::os::unix::fs::symlink(fixture.0.join("missing"), &paths[0]).unwrap();
    assert!(fixture.capture(&paths).is_err());
}
