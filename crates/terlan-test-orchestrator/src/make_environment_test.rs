use super::*;

fn command() -> Command {
    let mut command = Command::new("unused");
    command
        .env_clear()
        .current_dir(std::env::current_dir().unwrap())
        .env("PATH", "/bin");
    command
}

fn observed_identity(command: &Command) -> Result<String, PhaseFailure> {
    Ok(test_observation(command)?.digest)
}

#[test]
fn make_bookkeeping_changes_do_not_erase_runtime_or_compiler_inputs() {
    let baseline = observed_identity(&command()).unwrap();
    let mut make = command();
    make.env("MAKELEVEL", "2")
        .env("SHLVL", "3")
        .env("MAKEFLAGS", "w --jobserver-auth=3,4")
        .env("MAKE_TERMERR", "/dev/pts/2")
        .env("MAKE_TERMOUT", "/dev/pts/2")
        .env(CONTEXT, "live-owner")
        .env("TERLC", "/bound/terlc")
        .env("TERLAN_RUST_ORCHESTRATOR", "/bound/orchestrator")
        .env("TERLAN_CARGO_METADATA_OWNER", "/bound/metadata-owner")
        .env("TERLAN_RUST_SUITE_TERLC", "/bound/test-programs/terlc")
        .env("TERLAN_RUST_SUITE_VM", "/bound/test-programs/terlan-vm")
        .env(
            "TERLAN_RUST_SUITE_NATIVE_WORKER",
            "/bound/test-programs/terlan-native-worker",
        )
        .env("TERLAN_VALIDATION_BOOTSTRAPPED", "1")
        .env("TERLAN_PROOF_RUNTIME_RUN", ":")
        .env("TERLAN_PROOF_TRACK_RUN", ":");
    assert_eq!(observed_identity(&make).unwrap(), baseline);
    for scope in ["current-inputs", "hosted-source"] {
        make.env(SCOPE, scope);
        assert_eq!(observed_identity(&make).unwrap(), baseline);
    }
    make.env(SCOPE, "unchecked");
    assert!(observed_identity(&make).is_err());
    make.env_remove(SCOPE);
    for key in ["TERLAN_SEMANTIC_KERNEL_ROOT", "TERLAN_LEAN_PROOF_ROOT"] {
        make.env(key, std::env::current_dir().unwrap());
        assert_eq!(observed_identity(&make).unwrap(), baseline);
        make.env(key, "/another-checkout");
        assert!(observed_identity(&make).is_err());
        make.env_remove(key);
    }
    for key in [
        "RUSTFLAGS",
        "CARGO_HOME",
        "TERLAN_TEST_CAPABILITY_WORKER",
        "TERLAN_NATIVE_WORKER",
        "UNCLASSIFIED_OPTION",
    ] {
        let mut changed = command();
        changed.env(key, "changed");
        assert_ne!(observed_identity(&changed).unwrap(), baseline, "{key}");
    }
    let mut duplicated_test_path = command();
    duplicated_test_path.env("PATH", "/bin:/bin");
    assert_ne!(
        observed_identity(&duplicated_test_path).unwrap(),
        baseline,
        "proof children must preserve the coverage owner's exact PATH"
    );
    make.env("MAKELEVEL", "invalid");
    assert!(observed_identity(&make).is_err());
}

#[test]
fn environment_components_explain_differences_without_disclosing_inputs() {
    let baseline = test_observation(&command()).unwrap();
    let mut changed = command();
    changed.env("RUSTFLAGS", "secret-value");
    let changed = test_observation(&changed).unwrap();
    let difference = baseline.difference(&changed);
    assert!(difference.contains("extra="));
    assert!(!difference.contains("RUSTFLAGS"));
    assert!(!difference.contains("secret-value"));
    assert_eq!(TestIdentity::read(&baseline.json()).unwrap(), baseline);
    for malformed in [
        json!({}),
        json!({"scope":"test-environment-components-v1",
        "identity_sha256":"short","components":{}}),
    ] {
        assert!(TestIdentity::read(&malformed).is_err());
    }
}

#[test]
fn recursive_make_planning_is_not_confused_with_long_options_or_assignments() {
    for (flags, expected) in [
        ("n --no-print-directory", true),
        ("knw", true),
        ("-n", true),
        ("--dry-run", true),
        ("--just-print", true),
        ("--recon", true),
        ("--no-print-directory", false),
        ("", false),
        ("w -- VAR=contains-n", false),
        ("NAME=n", false),
    ] {
        let environment = ExecutionEnvironment::from_entries(
            vec![("MAKEFLAGS".into(), flags.into())],
            &std::env::current_dir().unwrap(),
        )
        .unwrap();
        assert_eq!(dry_run(&environment), expected, "{flags}");
    }
}

#[test]
fn invocation_identity_preserves_program_arguments_and_boundaries() {
    let baseline = invocation_identity(Command::new("make").args(["check", "ab", "c"]));
    assert_eq!(
        baseline,
        invocation_identity(Command::new("make").args(["check", "ab", "c"]))
    );
    for command in [
        Command::new("make").args(["release-evidence-compose", "ab", "c"]),
        Command::new("make").args(["check", "a", "bc"]),
        Command::new("make").args(["check", "c", "ab"]),
        Command::new("gmake").args(["check", "ab", "c"]),
    ] {
        assert_ne!(baseline, invocation_identity(command));
    }
}
