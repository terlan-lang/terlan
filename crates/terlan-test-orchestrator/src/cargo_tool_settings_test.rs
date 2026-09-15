use super::*;
use crate::test_orchestrator_test::temporary_fixture;
#[cfg(unix)]
use std::ffi::OsString;

fn settings(path: &Path, contents: &str) -> CargoToolSettings {
    CargoToolSettings::from_document(path, &toml::from_str(contents).unwrap()).unwrap()
}

fn environment(root: &Path, values: &[(&str, &str)]) -> ExecutionEnvironment {
    ExecutionEnvironment::from_entries(
        values
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into())),
        root,
    )
    .unwrap()
}

#[test]
fn direct_environment_overrides_cargo_environment_then_file_settings() {
    let fixture = temporary_fixture("cargo-tool-precedence");
    let settings = settings(&fixture.0.join(".cargo/config.toml"), "[build]\nrustc = './file-compiler'\nrustc-wrapper = 'wrapper'\nrustc-workspace-wrapper = 'workspace-wrapper'\nrustdoc = './documentation'\n");
    for (values, compiler) in [
        (vec![], "file-compiler"),
        (
            vec![("CARGO_BUILD_RUSTC", "./cargo-compiler")],
            "cargo-compiler",
        ),
        (
            vec![
                ("CARGO_BUILD_RUSTC", "./cargo-compiler"),
                ("RUSTC", "./direct-compiler"),
            ],
            "direct-compiler",
        ),
    ] {
        let selected = settings
            .programs(&environment(&fixture.0, &values))
            .unwrap();
        assert_eq!(selected[0], ("RUSTC", fixture.0.join(compiler)));
        assert_eq!(selected[1], ("RUSTDOC", fixture.0.join("documentation")));
        assert_eq!(selected[2], ("RUSTC_WRAPPER", "wrapper".into()));
        assert_eq!(
            selected[3],
            ("RUSTC_WORKSPACE_WRAPPER", "workspace-wrapper".into())
        );
    }
}

#[test]
fn empty_wrappers_disable_both_layers_but_empty_compilers_are_errors() {
    let fixture = temporary_fixture("cargo-tool-empty");
    let settings = settings(
        &fixture.0.join(".cargo/config.toml"),
        "[build]\nrustc-wrapper = 'wrapper'\nrustc-workspace-wrapper = 'workspace'\n",
    );
    for keys in [
        ["RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"],
        [
            "CARGO_BUILD_RUSTC_WRAPPER",
            "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        ],
    ] {
        let environment = environment(&fixture.0, &[(keys[0], ""), (keys[1], "")]);
        assert!(settings.programs(&environment).unwrap().is_empty());
    }
    for key in [
        "RUSTC",
        "RUSTDOC",
        "CARGO_BUILD_RUSTC",
        "CARGO_BUILD_RUSTDOC",
    ] {
        assert!(settings
            .programs(&environment(&fixture.0, &[(key, "")]))
            .is_err());
    }
}

#[test]
fn config_environment_does_not_select_tools_and_only_force_overrides_inheritance() {
    let fixture = temporary_fixture("cargo-tool-env");
    let settings = settings(&fixture.0.join(".cargo/config.toml"), "[build]\nrustc = './compiler'\n[env]\nRUSTC = 'not-the-selection'\nPRESENT = 'configured'\nFORCED = {value = 'configured', force = true}\nRELATIVE = {value = 'assets', relative = true}\nPATH = 'ignored-because-cargo-has-a-path'\n");
    let environment = environment(
        &fixture.0,
        &[("PRESENT", "inherited"), ("FORCED", "inherited")],
    );
    assert_eq!(
        settings.programs(&environment).unwrap(),
        [("RUSTC", fixture.0.join("compiler"))]
    );
    let command = settings.subprocess_command(&environment).unwrap();
    assert_eq!(
        command_value(&command, "RUSTC"),
        Some("not-the-selection".into())
    );
    assert_eq!(command_value(&command, "PRESENT"), Some("inherited".into()));
    assert_eq!(command_value(&command, "FORCED"), Some("configured".into()));
    assert_eq!(
        command_value(&command, "RELATIVE"),
        Some(fixture.0.join("assets").into_os_string())
    );
    assert_eq!(
        command_value(&command, "PATH"),
        Some(environment.test_path().to_owned())
    );
    assert!(environment.value("RELATIVE").is_none());
}

#[test]
fn field_merging_retains_cargos_table_origin_and_nearer_root_priority() {
    let fixture = temporary_fixture("cargo-tool-origin");
    let early = fixture.0.join("ancestor/.cargo/early.toml");
    let late = fixture.0.join("project/.cargo/late.toml");
    let mut included = settings(&early, "[env]\nASSET = {value = 'old', relative = true}\n");
    included
        .merge(
            settings(&late, "[env]\nASSET = {value = 'new', force = true}\n"),
            true,
        )
        .unwrap();
    let environment = environment(&fixture.0, &[]);
    let command = included.subprocess_command(&environment).unwrap();
    assert_eq!(
        command_value(&command, "ASSET"),
        Some(fixture.0.join("ancestor/new").into_os_string())
    );
    let mut nearer = settings(&late, "[env]\nASSET = {value = 'near'}\n");
    nearer.merge(included, false).unwrap();
    let command = nearer.subprocess_command(&environment).unwrap();
    assert_eq!(
        command_value(&command, "ASSET"),
        Some(fixture.0.join("project/near").into_os_string())
    );
}

#[test]
fn invalid_tool_environment_configuration_is_rejected_without_values_in_errors() {
    let fixture = temporary_fixture("cargo-tool-invalid");
    let path = fixture.0.join(".cargo/config.toml");
    let environment = environment(&fixture.0, &[("RUSTC", "override")]);
    for source in [
        "[build]\nrustc = 1",
        "[build]\nrustdoc = []",
        "[build]\nrustc-wrapper = {}",
    ] {
        assert!(settings(&path, source).programs(&environment).is_err());
    }
    for source in [
        "[env]\nCARGO_HOME = 'private-secret'",
        "[env]\nRUSTUP_HOME = 'private-secret'",
        "[env]\nRUSTUP_TOOLCHAIN = 'private-secret'",
        "[env]\nBAD = { force = true }",
        "[env]\nBAD = {value = 'private-secret', force = 1}",
        "[env]\n'BAD=NAME' = 'private-secret'",
    ] {
        let error = settings(&path, source)
            .subprocess_command(&environment)
            .err()
            .unwrap();
        assert!(!error.detail.contains("private-secret"));
    }
    let mut mixed = settings(&path, "[env]\nKEY = 'simple'");
    assert!(mixed
        .merge(settings(&path, "[env]\nKEY = {value = 'table'}"), true)
        .is_err());
    let too_many = (0..4097)
        .map(|index| format!("KEY{index} = 'value'\n"))
        .collect::<String>();
    let document: toml::Value = toml::from_str(&format!("[env]\n{too_many}")).unwrap();
    assert!(CargoToolSettings::from_document(&path, &document).is_err());
}

#[cfg(unix)]
#[test]
fn non_utf8_tool_environment_is_ignored_like_cargo_without_lossy_conversion() {
    use std::os::unix::ffi::OsStringExt;
    let fixture = temporary_fixture("cargo-tool-native-env");
    let settings = settings(
        &fixture.0.join(".cargo/config.toml"),
        "[build]\nrustc = 'configured'\n",
    );
    let environment = ExecutionEnvironment::from_entries(
        [
            ("RUSTC".into(), OsString::from_vec(vec![0xff])),
            ("CARGO_BUILD_RUSTC".into(), OsString::from_vec(vec![0xfe])),
        ],
        &fixture.0,
    )
    .unwrap();
    assert_eq!(
        settings.programs(&environment).unwrap(),
        [("RUSTC", "configured".into())]
    );
}
