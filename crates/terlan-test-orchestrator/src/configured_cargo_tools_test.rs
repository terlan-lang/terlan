use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use std::fs;
use std::path::Path;
use std::time::Duration;

fn control() -> ProcessControl<'static> {
    ProcessControl::new(Duration::from_secs(10))
}

fn environment(root: &Path, entries: &[(&str, OsString)]) -> ExecutionEnvironment {
    ExecutionEnvironment::from_entries(
        entries
            .iter()
            .map(|(key, value)| ((*key).into(), value.clone())),
        root,
    )
    .unwrap()
}

fn settings(root: &Path, source: &str) -> CargoToolSettings {
    CargoToolSettings::from_document(
        &root.join(".cargo/config.toml"),
        &toml::from_str(source).unwrap(),
    )
    .unwrap()
}

fn executable(path: &Path, source: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn no_overrides_still_require_a_single_closeout_and_honor_cancellation() {
    let fixture = temporary_fixture("cargo-tools-none");
    let settings = CargoToolSettings::default();
    let environment = environment(&fixture.0, &[]);
    let mut tools = ConfiguredCargoTools::capture(&settings, &environment, control()).unwrap();
    assert!(tools.is_bound());
    assert_eq!(
        tools.json()["environment_observation_scope"],
        "frozen-input-plus-cargo-env-and-selected-proxy-path"
    );
    assert!(!tools.verified());
    tools.verify(control()).unwrap();
    assert!(tools.verified());
    assert!(tools.verify(control()).is_err());
    let cancelled = std::sync::atomic::AtomicBool::new(true);
    assert_eq!(
        ConfiguredCargoTools::capture(
            &settings,
            &environment,
            control().with_cancellation(&cancelled)
        )
        .err()
        .unwrap()
        .outcome,
        "cancelled"
    );
}

#[test]
fn configured_compiler_wrapper_bytes_and_path_shadowing_are_bound() {
    let fixture = temporary_fixture("cargo-tools-mutation");
    let compiler = fixture
        .0
        .join(format!("later/compiler{}", std::env::consts::EXE_SUFFIX));
    let wrapper = fixture
        .0
        .join(format!("wrapper{}", std::env::consts::EXE_SUFFIX));
    executable(&compiler, "compiler one");
    executable(&wrapper, "wrapper one");
    fs::create_dir(fixture.0.join("earlier")).unwrap();
    let path = std::env::join_paths([fixture.0.join("earlier"), fixture.0.join("later")]).unwrap();
    let environment = environment(
        &fixture.0,
        &[("PATH", path), ("RUSTC_WRAPPER", wrapper.into_os_string())],
    );
    let settings = settings(&fixture.0, "[build]\nrustc = 'compiler'\n");
    let mut changed = ConfiguredCargoTools::capture(&settings, &environment, control()).unwrap();
    let modified = fs::metadata(&compiler).unwrap().modified().unwrap();
    fs::write(&compiler, "compiler two").unwrap();
    fs::File::options()
        .write(true)
        .open(&compiler)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    assert!(changed.verify(control()).is_err());
    assert!(!changed.verified());
    let mut shadowed = ConfiguredCargoTools::capture(&settings, &environment, control()).unwrap();
    executable(
        &fixture
            .0
            .join(format!("earlier/compiler{}", std::env::consts::EXE_SUFFIX)),
        "compiler new",
    );
    assert!(shadowed.verify(control()).is_err());
    assert_eq!(shadowed.json()["paths_verified"], false);
}

#[test]
fn config_forced_path_resolves_tools_without_changing_the_cargo_environment() {
    let fixture = temporary_fixture("cargo-tools-path");
    let compiler = fixture
        .0
        .join(format!("tools/compiler{}", std::env::consts::EXE_SUFFIX));
    executable(&compiler, "compiler");
    let environment = environment(&fixture.0, &[("PATH", "missing".into())]);
    let settings = settings(&fixture.0, "[build]\nrustc = 'compiler'\n[env]\nPATH = {value = 'tools', relative = true, force = true}\nPRIVATE = 'private-secret'\n");
    let mut tools = ConfiguredCargoTools::capture(&settings, &environment, control()).unwrap();
    assert_eq!(tools.resolved, [("RUSTC", compiler)]);
    assert_eq!(environment.value("PATH"), Some("missing".into()));
    assert!(!tools.json().to_string().contains("private-secret"));
    tools.verify(control()).unwrap();
    assert!(tools.verified());
}

#[cfg(unix)]
#[test]
fn real_cargo_matches_declared_tool_chain_and_effective_environment() {
    use crate::tool_configuration::ToolConfiguration;
    use std::process::Command;
    let fixture = temporary_fixture("cargo-tools-real");
    fs::create_dir_all(fixture.0.join(".cargo/deep")).unwrap();
    fs::create_dir(fixture.0.join("src")).unwrap();
    fs::write(fixture.0.join("Cargo.toml"), "[package]\nname = 'tool_selection_fixture'\nversion = '0.0.0'\nedition = '2024'\n[workspace]\n").unwrap();
    fs::write(
        fixture.0.join("Cargo.lock"),
        "version = 4\n[[package]]\nname = 'tool_selection_fixture'\nversion = '0.0.0'\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("src/lib.rs"),
        "// Compiler selection must stop before compilation.\n",
    )
    .unwrap();
    for compiler in ["configured", "cargo-env", "direct-env"] {
        executable(&fixture.0.join(format!("tools/{compiler}")), &format!("#!/bin/sh\nprintf 'compiler:{compiler}:%s:%s\\n' \"$FIXTURE_VALUE\" \"$FIXTURE_RELATIVE\" >> \"$FIXTURE_OUTPUT\"\nexit 89\n"));
    }
    for wrapper in ["outer", "workspace"] {
        executable(
            &fixture.0.join(format!("tools/{wrapper}")),
            &format!(
                "#!/bin/sh\nprintf 'wrapper:{wrapper}\\n' >> \"$FIXTURE_OUTPUT\"\nexec \"$@\"\n"
            ),
        );
    }
    let relative = fixture.0.join("assets");
    for (index, case) in [
        "configured",
        "cargo-env",
        "direct-env",
        "empty-wrapper",
        "forced-path",
        "non-utf8",
        "include-origin",
    ]
    .into_iter()
    .enumerate()
    {
        let output = fixture.0.join(format!("observed-{index}"));
        let mut entries = vec![
            ("CARGO_HOME", fixture.0.join("cargo-home").into_os_string()),
            (
                "RUSTUP_HOME",
                fixture.0.join("rustup-home").into_os_string(),
            ),
            ("PATH", "/usr/bin:/bin".into()),
            ("FIXTURE_OUTPUT", output.clone().into_os_string()),
            ("FIXTURE_VALUE", "inherited".into()),
        ];
        if matches!(case, "cargo-env" | "direct-env") {
            entries.push((
                "CARGO_BUILD_RUSTC",
                fixture.0.join("tools/cargo-env").into_os_string(),
            ));
        }
        if case == "direct-env" {
            entries.push(("RUSTC", fixture.0.join("tools/direct-env").into_os_string()));
        }
        if case == "empty-wrapper" {
            entries.extend([
                ("RUSTC_WRAPPER", "".into()),
                ("RUSTC_WORKSPACE_WRAPPER", "".into()),
            ]);
        }
        if case == "non-utf8" {
            use std::os::unix::ffi::OsStringExt;
            entries.extend([
                ("RUSTC", OsString::from_vec(vec![0xff])),
                ("CARGO_BUILD_RUSTC", OsString::from_vec(vec![0xfe])),
            ]);
        }
        let force = case == "forced-path";
        let compiler = if force {
            "configured"
        } else {
            "./tools/configured"
        };
        let wrapper = if force { "outer" } else { "./tools/outer" };
        let workspace = if force {
            "workspace"
        } else {
            "./tools/workspace"
        };
        let mut source = format!("[build]\nrustc = '{compiler}'\nrustc-wrapper = '{wrapper}'\nrustc-workspace-wrapper = '{workspace}'\n[env]\nFIXTURE_VALUE = {{value = 'configured', force = {force}}}\nFIXTURE_RELATIVE = {{value = 'assets', relative = true}}\nRUSTC = 'not-a-tool-selection'\n");
        if force {
            source.push_str("PATH = {value = 'tools', relative = true, force = true}\n");
        }
        if case == "include-origin" {
            fs::write(
                fixture.0.join(".cargo/deep/env.toml"),
                "[env]\nFIXTURE_RELATIVE = {value = 'old', relative = true}\n",
            )
            .unwrap();
            source = format!("include = ['deep/env.toml']\n{source}");
        }
        fs::write(fixture.0.join(".cargo/config.toml"), source).unwrap();
        let environment = environment(&fixture.0, &entries);
        let mut configuration = ToolConfiguration::capture(&environment, control()).unwrap();
        let settings = configuration.cargo_settings();
        let expected_command = settings.subprocess_command(&environment).unwrap();
        let mut tools = ConfiguredCargoTools::capture(settings, &environment, control()).unwrap();
        let expected_compiler = if matches!(case, "cargo-env" | "direct-env") {
            case
        } else {
            "configured"
        };
        assert_eq!(
            tools
                .resolved
                .iter()
                .find(|(role, _)| *role == "RUSTC")
                .unwrap()
                .1
                .file_name()
                .unwrap(),
            expected_compiler
        );
        let expected_relative = if case == "include-origin" {
            fixture.0.join(".cargo/assets")
        } else {
            relative.clone()
        };
        assert_eq!(
            command_value(&expected_command, "FIXTURE_RELATIVE"),
            Some(expected_relative.clone().into_os_string())
        );
        let mut command = Command::new(crate::cargo_program());
        command
            .current_dir(&fixture.0)
            .env_clear()
            .envs(entries.iter().map(|(key, value)| (key, value)))
            .args(["check", "--offline", "--locked"]);
        let mut launches = 0;
        assert!(control()
            .run(&mut command, |_| {
                launches += 1;
                Ok(())
            })
            .is_err());
        assert_eq!(launches, 1);
        let expected_wrappers = if case == "empty-wrapper" {
            ""
        } else {
            "wrapper:outer\nwrapper:workspace\n"
        };
        let expected_value = if force { "configured" } else { "inherited" };
        assert_eq!(
            fs::read_to_string(output).unwrap(),
            format!(
                "{expected_wrappers}compiler:{expected_compiler}:{expected_value}:{}\n",
                expected_relative.display()
            ),
            "{case}"
        );
        tools.verify(control()).unwrap();
        configuration.verify(control()).unwrap();
        assert!(tools.verified() && configuration.verified());
    }
}

#[test]
fn suite_requires_configured_tool_closeout_and_persists_mutation_failure() {
    use crate::launch_ledger::LaunchLedger;
    use crate::ValidationTier;
    let fixture = temporary_fixture("cargo-tools-ledger");
    for name in ["rustc", "rustdoc"] {
        executable(
            &fixture
                .0
                .join("target/debug")
                .join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
            "default tool",
        );
    }
    let wrapper = fixture
        .0
        .join(format!("wrapper{}", std::env::consts::EXE_SUFFIX));
    let control = control();
    let environment = environment(
        &fixture.0,
        &[
            ("CARGO_HOME", fixture.0.join("cargo-home").into_os_string()),
            (
                "RUSTUP_HOME",
                fixture.0.join("rustup-home").into_os_string(),
            ),
            ("RUSTC_WRAPPER", wrapper.clone().into_os_string()),
        ],
    );
    for mode in ["matching", "changed", "missing", "repeated", "late"] {
        executable(&wrapper, "wrapper one");
        let report = fixture.0.join(format!("{mode}.json"));
        let mut ledger = LaunchLedger::new(&report, 1, Duration::from_secs(10)).unwrap();
        ledger.bind_environment(&environment, &[]).unwrap();
        ledger.admit_configuration(&environment, control).unwrap();
        if mode != "late" {
            ledger.admit_cargo_tools(&environment, control).unwrap();
        }
        if mode == "repeated" {
            assert!(ledger.admit_cargo_tools(&environment, control).is_err());
        } else {
            ledger
                .execute(
                    "fixture observation",
                    ValidationTier::FastUnit,
                    "fixture-tool",
                    |launched| {
                        control
                            .run(std::process::Command::new("git").arg("--version"), launched)
                            .map_err(crate::process_failure)
                    },
                )
                .unwrap();
            if mode == "late" {
                assert!(ledger.admit_cargo_tools(&environment, control).is_err());
            } else {
                ledger.verify_configuration(control).unwrap();
                if mode == "changed" {
                    fs::write(&wrapper, "wrapper two").unwrap();
                }
                if mode != "missing" {
                    assert_eq!(
                        ledger.verify_cargo_tools(control).is_ok(),
                        mode == "matching"
                    );
                }
            }
        }
        assert_eq!(ledger.finish().is_ok(), mode == "matching");
        let report: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
        assert_eq!(
            report["decision"],
            if mode == "matching" { "pass" } else { "fail" }
        );
        if mode == "changed" {
            assert_eq!(report["cargo_tool_binding"]["verified"], false);
        }
    }
}
