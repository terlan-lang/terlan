use super::*;
use crate::execution_environment::command_value;
use crate::test_orchestrator_test::temporary_fixture;

#[test]
fn proxy_environment_precedes_config_and_cargo_identity_wins_last() {
    let fixture = temporary_fixture("compiler-environment");
    let root = fixture.0.join("toolchain");
    let native = root.join("bin/cargo");
    let environment = ExecutionEnvironment::from_entries(
        [
            (
                "CARGO_HOME".into(),
                fixture.0.join("cargo-home").into_os_string(),
            ),
            (
                "RUSTUP_HOME".into(),
                fixture.0.join("rustup-home").into_os_string(),
            ),
            ("RUSTUP_TOOLCHAIN".into(), "inherited".into()),
            ("RUST_RECURSION_COUNT".into(), "3".into()),
            ("PATH".into(), "/usr/bin:/bin".into()),
        ],
        &fixture.0,
    )
    .unwrap();
    let loader = if cfg!(target_os = "macos") {
        "DYLD_FALLBACK_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    };
    for force in [true, false] {
        let source = format!("[env]\n{loader} = {{value = 'configured-loader', force = {force}}}\nRUST_RECURSION_COUNT = {{value = '7', force = {force}}}\nCARGO = {{value = 'incorrect-cargo', force = true}}\n");
        let settings = CargoToolSettings::from_document(
            &fixture.0.join(".cargo/config.toml"),
            &toml::from_str(&source).unwrap(),
        )
        .unwrap();
        let selection = RustupSelection {
            name: "actual".into(),
            source: "toolchain-file",
        };
        let command = capture(&settings, &environment, Some((&root, &selection)), &native).unwrap();
        assert_eq!(
            command_value(&command, "RUSTUP_TOOLCHAIN"),
            Some("actual".into())
        );
        assert_eq!(
            command_value(&command, "RUSTUP_TOOLCHAIN_SOURCE"),
            Some("toolchain-file".into())
        );
        assert_eq!(
            command_value(&command, "CARGO"),
            Some(native.clone().into_os_string())
        );
        assert_eq!(
            command_value(&command, "RUST_RECURSION_COUNT"),
            Some(if force { "7" } else { "4" }.into())
        );
        let actual = command_value(&command, loader).unwrap();
        if force {
            assert_eq!(actual, "configured-loader");
        } else {
            assert_eq!(
                std::env::split_paths(&actual).next(),
                Some(root.join("lib"))
            );
        }
    }
    assert_eq!(environment.value("RUST_RECURSION_COUNT"), Some("3".into()));
}

#[cfg(unix)]
#[test]
fn actual_native_and_proxy_cargo_version_environments_match_the_probe_context() {
    use crate::executable_binding::resolve_program;
    use std::collections::BTreeMap;
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::time::Duration;
    use terlan_process_owner::ProcessControl;
    let control = ProcessControl::new(Duration::from_secs(20));
    let rustup = resolve_program("rustup", &std::env::var_os("PATH").unwrap()).unwrap();
    let output = control
        .capture_stdout(
            Command::new(&rustup).args(["which", "cargo"]),
            64 * 1024,
            |_| Ok(()),
        )
        .unwrap();
    let native = PathBuf::from(std::str::from_utf8(&output).unwrap().trim());
    let root = native.parent().unwrap().parent().unwrap();
    for (proxied, force) in [(false, false), (true, false), (true, true)] {
        let fixture = temporary_fixture("compiler-environment-real");
        fs::create_dir_all(fixture.0.join(".cargo")).unwrap();
        fs::create_dir(fixture.0.join("src")).unwrap();
        fs::write(fixture.0.join("Cargo.toml"), "[package]\nname = 'compiler_environment_fixture'\nversion = '0.0.0'\nedition = '2024'\n[workspace]\n").unwrap();
        fs::write(
            fixture.0.join("Cargo.lock"),
            "version = 4\n[[package]]\nname = 'compiler_environment_fixture'\nversion = '0.0.0'\n",
        )
        .unwrap();
        fs::write(
            fixture.0.join("src/lib.rs"),
            "// Must stop at compiler query.\n",
        )
        .unwrap();
        let compiler = fixture.0.join("compiler");
        fs::write(
            &compiler,
            "#!/bin/sh\n/usr/bin/env > \"$FIXTURE_TRACE\"\nexit 89\n",
        )
        .unwrap();
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o755)).unwrap();
        let source = format!("[env]\nLD_LIBRARY_PATH = {{value = 'configured-loader', force = {force}}}\nDYLD_FALLBACK_LIBRARY_PATH = {{value = 'configured-loader', force = {force}}}\nCARGO = {{value = 'incorrect-cargo', force = true}}\n");
        let config = fixture.0.join(".cargo/config.toml");
        fs::write(&config, &source).unwrap();
        let settings =
            CargoToolSettings::from_document(&config, &toml::from_str(&source).unwrap()).unwrap();
        let environment = ExecutionEnvironment::from_entries(
            [
                ("SSL_CERT_FILE".into(), config.clone().into_os_string()),
                (
                    "SSL_CERT_DIR".into(),
                    fixture.0.join(".cargo").into_os_string(),
                ),
                (
                    "CARGO_HOME".into(),
                    fixture.0.join("cargo-home").into_os_string(),
                ),
                (
                    "RUSTUP_HOME".into(),
                    std::env::var_os("RUSTUP_HOME").unwrap(),
                ),
                (
                    "RUSTUP_TOOLCHAIN".into(),
                    root.file_name().unwrap().to_owned(),
                ),
                ("RUST_RECURSION_COUNT".into(), "3".into()),
                ("RUSTC".into(), compiler.into_os_string()),
                (
                    "FIXTURE_TRACE".into(),
                    fixture.0.join("trace").into_os_string(),
                ),
                ("PATH".into(), "/usr/bin:/bin".into()),
            ],
            &fixture.0,
        )
        .unwrap();
        let proxy = fixture.0.join("cargo");
        symlink(&rustup, &proxy).unwrap();
        let mut command = environment.test_command(if proxied { &proxy } else { &native });
        command.args(["test", "--offline", "--locked", "--no-run", "--lib"]);
        assert!(control
            .capture_stdout(&mut command, 64 * 1024, |_| Ok(()))
            .is_err());
        let actual = fs::read_to_string(fixture.0.join("trace")).unwrap();
        let actual = actual
            .lines()
            .map(|line| line.split_once('=').unwrap())
            .filter(|(key, _)| !["PWD", "SHLVL", "_"].contains(key))
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect::<BTreeMap<_, _>>();
        let selection = RustupSelection {
            name: root.file_name().unwrap().to_str().unwrap().into(),
            source: "env",
        };
        let observed = capture(
            &settings,
            &environment,
            proxied.then_some((root, &selection)),
            &fs::canonicalize(&native).unwrap(),
        )
        .unwrap();
        let expected = observed
            .get_envs()
            .filter_map(|(key, value)| {
                Some((key.to_str()?.to_owned(), value?.to_str()?.to_owned()))
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(actual, expected, "proxy={proxied}, force={force}");
    }
}
