//! Actual Cargo/libtest selections must not accept missing or ignored test bodies.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-library-selection-{}-{time}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"terlan\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[features]\nnative-codegen = []\n").unwrap();
        fs::write(
            root.join("Cargo.lock"),
            "version = 4\n[[package]]\nname = \"terlan\"\nversion = \"0.0.0\"\n",
        )
        .unwrap();
        fs::write(
            root.join("src/lib.rs"),
            r#"
#[cfg(test)]
mod selected {
    #[test] fn first() { assert_eq!(1 + 1, 2); }
    #[test] fn second() { assert_eq!(2 + 2, 4); }
    #[test] #[ignore = "requires a separate owner"] fn ignored() {}
    #[test] fn failing() { panic!("intentional fixture failure"); }
}
"#,
        )
        .unwrap();
        Self(root)
    }

    fn run(&self, names: &[&str]) -> bool {
        let mut command = Command::new(env!("CARGO_BIN_EXE_terlan-test-orchestrator"));
        command
            .current_dir(&self.0)
            .args(["--run-library-tests", "--target-dir"])
            .arg(self.0.join("target"))
            .arg("--")
            .args(names)
            .env("CARGO_NET_OFFLINE", "true");
        let output = ProcessControl::new(Duration::from_secs(30))
            .capture_stdout_result(&mut command, 1024 * 1024, |_| Ok(()))
            .unwrap();
        let logs = self.0.join("target/quality");
        if logs.exists() {
            assert!(
                fs::read_dir(logs).unwrap().next().is_none(),
                "private result logs were not cleaned"
            );
        }
        if output.outcome.is_ok() {
            assert!(String::from_utf8(output.stdout)
                .unwrap()
                .contains("verified 2 exact library tests"));
        }
        output.outcome.is_ok()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove only this disposable Cargo fixture");
    }
}

#[test]
fn real_library_selection_requires_every_requested_test_to_pass() {
    let fixture = Fixture::new();
    assert!(fixture.run(&["selected::first", "selected::second"]));
    assert!(!fixture.run(&["missing::test"]));
    assert!(!fixture.run(&["selected::first", "missing::test"]));
    assert!(!fixture.run(&["selected::first", "selected::ignored"]));
    assert!(!fixture.run(&["selected::first", "selected::failing"]));
}

#[test]
fn invalid_library_selection_does_not_start_cargo() {
    let fixture = Fixture::new();
    for names in [
        vec![],
        vec!["--list"],
        vec!["selected::first", "selected::first"],
    ] {
        assert!(!fixture.run(&names));
        assert!(!fixture.0.join("target").exists());
    }
}
