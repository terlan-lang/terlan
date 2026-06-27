use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_erlang_modernization_inventory, run_erlang_runtime_matrix};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[test]
fn erlang_modernization_inventory_emits_required_artifacts() {
    let sandbox = TestSandbox::new("em0_inventory_emits");
    sandbox.create_reference_vm(&[
        "common_test",
        "compiler",
        "kernel",
        "parsetools",
        "sasl",
        "stdlib",
        "syntax_tools",
        "tools",
    ]);

    let summary = run_erlang_modernization_inventory(&sandbox.repo).unwrap();

    assert_eq!(summary.artifact_count, 8);
    assert_eq!(summary.kept_app_count, 8);
    assert_eq!(summary.removed_app_count, 20);
    assert!(sandbox
        .repo
        .join("_build/erlang-modernization/reference-otp-baseline.json")
        .is_file());
    assert!(sandbox
        .repo
        .join("_build/erlang-modernization/reduced-otp-profile.md")
        .is_file());
}

#[test]
fn erlang_modernization_inventory_rejects_removed_otp_apps() {
    let sandbox = TestSandbox::new("em0_inventory_rejects_removed");
    sandbox.create_reference_vm(&[
        "common_test",
        "compiler",
        "kernel",
        "parsetools",
        "sasl",
        "stdlib",
        "syntax_tools",
        "tools",
        "observer",
    ]);

    let error = run_erlang_modernization_inventory(&sandbox.repo).unwrap_err();

    assert!(error.contains("observer"));
    assert!(error.contains("reduced VM still includes removed apps"));
}

#[test]
fn erlang_modernization_inventory_rejects_experimental_runtime_dependency() {
    let sandbox = TestSandbox::new("em0_inventory_rejects_runtime_dependency");
    sandbox.create_reference_vm(&[
        "common_test",
        "compiler",
        "kernel",
        "parsetools",
        "sasl",
        "stdlib",
        "syntax_tools",
        "tools",
    ]);
    fs::write(
        sandbox.repo.join("Cargo.toml"),
        "[dependencies]\nterlan-vm = { path = \"../terlan-vm\" }\n",
    )
    .unwrap();

    let error = run_erlang_modernization_inventory(&sandbox.repo).unwrap_err();

    assert!(error.contains("experimental runtime dependency"));
    assert!(error.contains("terlan-vm"));
}

#[test]
fn erlang_runtime_matrix_requires_otp29_bin() {
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let sandbox = TestSandbox::new("runtime_matrix_requires_otp29");
    std::env::remove_var("TERLAN_OTP29_BIN");
    std::env::remove_var("TERLAN_OTP_RUNTIME_BIN");
    std::env::set_var("TERLAN_RUNTIME_MATRIX_COMMAND", "true");

    let error = run_erlang_runtime_matrix(&sandbox.repo).unwrap_err();

    std::env::remove_var("TERLAN_RUNTIME_MATRIX_COMMAND");
    assert!(error.contains("requires TERLAN_OTP29_BIN"));
}

#[test]
fn erlang_runtime_matrix_requires_otp_runtime_bin() {
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let sandbox = TestSandbox::new("runtime_matrix_requires_otp_runtime");
    let otp29_bin = sandbox.fake_runtime_bin("otp29", "29");
    std::env::set_var("TERLAN_OTP29_BIN", &otp29_bin);
    std::env::remove_var("TERLAN_OTP_RUNTIME_BIN");
    std::env::set_var("TERLAN_RUNTIME_MATRIX_COMMAND", "true");

    let error = run_erlang_runtime_matrix(&sandbox.repo).unwrap_err();

    std::env::remove_var("TERLAN_OTP29_BIN");
    std::env::remove_var("TERLAN_RUNTIME_MATRIX_COMMAND");
    assert!(error.contains("requires TERLAN_OTP_RUNTIME_BIN"));
}

#[test]
fn erlang_runtime_matrix_runs_same_command_under_both_lanes() {
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let sandbox = TestSandbox::new("runtime_matrix_runs");
    let otp29_bin = sandbox.fake_runtime_bin("otp29", "29");
    let vm_bin = sandbox.fake_runtime_bin("terlan_vm", "30");
    let marker = sandbox.root.join("matrix-marker");
    std::env::set_var("TERLAN_OTP29_BIN", &otp29_bin);
    std::env::set_var("TERLAN_OTP_RUNTIME_BIN", &vm_bin);
    std::env::set_var(
        "TERLAN_RUNTIME_MATRIX_COMMAND",
        format!(
            "printf '%s\\n' \"$TERLAN_RUNTIME_MATRIX_LANE\" >> {}",
            marker.display()
        ),
    );

    let summary = run_erlang_runtime_matrix(&sandbox.repo).unwrap();

    std::env::remove_var("TERLAN_OTP29_BIN");
    std::env::remove_var("TERLAN_OTP_RUNTIME_BIN");
    std::env::remove_var("TERLAN_RUNTIME_MATRIX_COMMAND");
    assert_eq!(summary.lane_count, 2);
    assert_eq!(fs::read_to_string(marker).unwrap(), "otp29\notp-runtime\n");
}

struct TestSandbox {
    root: PathBuf,
    repo: PathBuf,
    vm: PathBuf,
}

impl TestSandbox {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("terlan_{name}_{nonce}"));
        let repo = root.join("terlan");
        let vm = root.join("terlan-vm");
        fs::create_dir_all(&repo).unwrap();
        Self { root, repo, vm }
    }

    fn create_reference_vm(&self, apps: &[&str]) {
        fs::create_dir_all(self.vm.join("lib")).unwrap();
        fs::write(self.vm.join("README.md"), "reference vm\n").unwrap();
        fs::write(self.vm.join("otp_versions.table"), "OTP\n").unwrap();
        for app in apps {
            fs::create_dir_all(self.vm.join("lib").join(app)).unwrap();
        }
    }

    fn fake_runtime_bin(&self, name: &str, otp_release: &str) -> PathBuf {
        let bin = self.root.join(name).join("bin");
        fs::create_dir_all(&bin).unwrap();
        write_executable(
            &bin.join("erl"),
            &format!("#!/usr/bin/env sh\nprintf '%s\\n' '{otp_release}'\n"),
        );
        write_executable(&bin.join("erlc"), "#!/usr/bin/env sh\nexit 0\n");
        bin
    }
}

#[cfg(unix)]
fn write_executable(path: &PathBuf, text: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, text).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

impl Drop for TestSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
