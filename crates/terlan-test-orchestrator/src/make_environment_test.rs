use super::*;

fn command() -> Command {
    let mut command = Command::new("unused");
    command
        .env_clear()
        .current_dir(std::env::current_dir().unwrap())
        .env("PATH", "/bin");
    command
}

#[test]
fn make_bookkeeping_changes_do_not_erase_runtime_or_compiler_inputs() {
    let baseline = test_identity(&command()).unwrap();
    let mut make = command();
    make.env("MAKELEVEL", "2")
        .env("SHLVL", "3")
        .env("MAKEFLAGS", "w --jobserver-auth=3,4")
        .env(CONTEXT, "live-owner")
        .env("TERLAN_VALIDATION_BOOTSTRAPPED", "1");
    assert_eq!(test_identity(&make).unwrap(), baseline);
    for scope in ["current-inputs", "hosted-source"] {
        make.env(SCOPE, scope);
        assert_eq!(test_identity(&make).unwrap(), baseline);
    }
    make.env(SCOPE, "unchecked");
    assert!(test_identity(&make).is_err());
    make.env_remove(SCOPE);
    for key in ["TERLAN_SEMANTIC_KERNEL_ROOT", "TERLAN_LEAN_PROOF_ROOT"] {
        make.env(key, std::env::current_dir().unwrap());
        assert_eq!(test_identity(&make).unwrap(), baseline);
        make.env(key, "/another-checkout");
        assert!(test_identity(&make).is_err());
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
        assert_ne!(test_identity(&changed).unwrap(), baseline, "{key}");
    }
    make.env("MAKELEVEL", "invalid");
    assert!(test_identity(&make).is_err());
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
