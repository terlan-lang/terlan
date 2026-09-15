use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use crate::tool_configuration::{observe_started, paths, ToolConfiguration};
use std::fs;
use std::time::Duration;

fn control() -> ProcessControl<'static> {
    ProcessControl::new(Duration::from_secs(10))
}

fn environment(root: &Path) -> ExecutionEnvironment {
    ExecutionEnvironment::from_entries(
        [
            (
                "CARGO_HOME".into(),
                root.join("cargo-home").into_os_string(),
            ),
            (
                "RUSTUP_HOME".into(),
                root.join("rustup-home").into_os_string(),
            ),
        ],
        root,
    )
    .unwrap()
}

fn write(root: &Path, relative: &str, contents: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, contents).unwrap();
    path
}

#[test]
fn active_hierarchy_and_recursive_includes_keep_cargo_precedence() {
    let fixture = temporary_fixture("cargo-include-order");
    let environment = environment(&fixture.0);
    let home = write(&fixture.0, "cargo-home/config.toml", "[build]\njobs = 1\n");
    let root = write(
        &fixture.0,
        ".cargo/config",
        "include = ['first.toml', 'second.toml']\n",
    );
    write(
        &fixture.0,
        ".cargo/config.toml",
        "invalid ignored configuration",
    );
    let first = write(
        &fixture.0,
        ".cargo/first.toml",
        "include = ['deep/third.toml']\n",
    );
    let third = write(&fixture.0, ".cargo/deep/third.toml", "[build]\njobs = 2\n");
    let second = write(&fixture.0, ".cargo/second.toml", "[build]\njobs = 3\n");
    let mut binding = ToolConfiguration::capture(&environment, control()).unwrap();
    assert_eq!(binding.cargo_order, [home, third, first, second, root]);
    assert!(!binding.verified());
    binding.verify(control()).unwrap();
    assert!(binding.verified());
}

#[test]
fn included_files_and_optional_absence_are_bound_at_closeout() {
    let fixture = temporary_fixture("cargo-include-change");
    let environment = environment(&fixture.0);
    write(
        &fixture.0,
        ".cargo/config.toml",
        "include = ['included.toml', { path = 'optional.toml', optional = true }]\n",
    );
    let included = write(
        &fixture.0,
        ".cargo/included.toml",
        "[build]\nrustc = 'first'\n",
    );
    let mut changed = ToolConfiguration::capture(&environment, control()).unwrap();
    let modified = fs::metadata(&included).unwrap().modified().unwrap();
    fs::write(&included, "[build]\nrustc = 'other'\n").unwrap();
    fs::File::options()
        .write(true)
        .open(&included)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    assert!(changed.verify(control()).is_err());
    let mut appeared = ToolConfiguration::capture(&environment, control()).unwrap();
    let optional = write(
        &fixture.0,
        ".cargo/optional.toml",
        "[build]\nrustc = 'third'\n",
    );
    assert!(appeared.verify(control()).is_err());
    assert!(appeared
        .before
        .iter()
        .any(|row| row.path == optional && !row.present));
    assert!(appeared
        .after
        .as_ref()
        .unwrap()
        .iter()
        .any(|row| row.path == optional && row.present));
}

#[test]
fn includes_parse_the_hashed_snapshot_not_a_second_live_read() {
    let fixture = temporary_fixture("cargo-include-snapshot");
    let environment = environment(&fixture.0);
    let root = write(
        &fixture.0,
        ".cargo/config.toml",
        "include = ['original.toml']\n",
    );
    let included = write(&fixture.0, ".cargo/original.toml", "[build]\njobs = 1\n");
    let started = Instant::now();
    let mut rows =
        observe_started(&paths(&environment).unwrap(), control(), MAX_BYTES, started).unwrap();
    fs::write(&root, "include = ['not-admitted.toml']\n").unwrap();
    let (order, settings) = expand(&environment, &mut rows, control(), started).unwrap();
    assert_eq!(order, [included, root]);
    let mut binding = ToolConfiguration {
        before: rows,
        after: None,
        cargo_order: order,
        settings,
    };
    binding
        .before
        .sort_by(|left, right| left.path.cmp(&right.path));
    assert!(binding.verify(control()).is_err());
}

#[test]
fn missing_repeated_cyclic_and_invalid_includes_fail_admission() {
    let fixture = temporary_fixture("cargo-include-reject");
    let environment = environment(&fixture.0);
    write(&fixture.0, ".cargo/ok.toml", "[build]\njobs = 1\n");
    for contents in [
        "include = ['missing.toml']",
        "include = ['config.toml']",
        "include = ['ok.toml', 'ok.toml']",
        "include = 'ok.toml'",
        "include = [17]",
        "include = [{ optional = true }]",
        "include = [{ path = 'ok.toml', optional = 'true' }]",
        "include = ['bad.txt']",
        "include = ['*.toml']",
        "include = ['{name}.toml']",
    ] {
        write(&fixture.0, ".cargo/config.toml", contents);
        assert!(
            ToolConfiguration::capture(&environment, control()).is_err(),
            "{contents}"
        );
    }
}

#[test]
fn malformed_configuration_diagnostics_do_not_echo_secrets() {
    let fixture = temporary_fixture("cargo-include-redaction");
    let environment = environment(&fixture.0);
    write(
        &fixture.0,
        ".cargo/config.toml",
        "include = ['secret.toml']\n",
    );
    let path = write(
        &fixture.0,
        ".cargo/secret.toml",
        "token = 'private-secret\n",
    );
    let error = ToolConfiguration::capture(&environment, control())
        .err()
        .unwrap();
    assert!(!error.detail.contains("private-secret"));
    fs::write(&path, "token = 'private-secret'\n").unwrap();
    let binding = ToolConfiguration::capture(&environment, control()).unwrap();
    assert!(!binding.json().to_string().contains("private-secret"));
}

#[test]
fn include_traversal_shares_byte_path_depth_and_cancellation_limits() {
    let fixture = temporary_fixture("cargo-include-budgets");
    let environment = environment(&fixture.0);
    write(
        &fixture.0,
        ".cargo/config.toml",
        "include = ['large.toml']\n",
    );
    let large = write(&fixture.0, ".cargo/large.toml", "");
    fs::File::options()
        .write(true)
        .open(&large)
        .unwrap()
        .set_len(MAX_BYTES)
        .unwrap();
    assert!(ToolConfiguration::capture(&environment, control()).is_err());
    for index in 0..65 {
        write(
            &fixture.0,
            &format!(".cargo/depth-{index}.toml"),
            &format!("include = ['depth-{}.toml']\n", index + 1),
        );
    }
    write(
        &fixture.0,
        ".cargo/config.toml",
        "include = ['depth-0.toml']\n",
    );
    assert!(ToolConfiguration::capture(&environment, control())
        .err()
        .unwrap()
        .detail
        .contains("traversal budget"));
    let cancelled = std::sync::atomic::AtomicBool::new(true);
    assert_eq!(
        ToolConfiguration::capture(&environment, control().with_cancellation(&cancelled))
            .err()
            .unwrap()
            .outcome,
        "cancelled"
    );
    let values = (0..MAX_PATHS)
        .map(|index| format!("{{path = 'optional-{index}.toml', optional = true}}"))
        .collect::<Vec<_>>()
        .join(",");
    write(
        &fixture.0,
        ".cargo/config.toml",
        &format!("include = [{values}]\n"),
    );
    assert!(ToolConfiguration::capture(&environment, control())
        .err()
        .unwrap()
        .detail
        .contains("path budget"));
}

#[cfg(unix)]
#[test]
fn include_symlinks_use_declaring_paths_and_reject_special_files() {
    use std::os::unix::fs::symlink;
    let fixture = temporary_fixture("cargo-include-links");
    let environment = environment(&fixture.0);
    let root = write(
        &fixture.0,
        ".cargo/config.toml",
        "include = ['link.toml']\n",
    );
    let target = write(
        &fixture.0,
        "external/target.toml",
        "include = ['relative.toml']\n",
    );
    let relative = write(&fixture.0, ".cargo/relative.toml", "[build]\njobs = 1\n");
    let link = fixture.0.join(".cargo/link.toml");
    symlink(&target, &link).unwrap();
    let mut binding = ToolConfiguration::capture(&environment, control()).unwrap();
    assert_eq!(binding.cargo_order, [relative, link.clone(), root]);
    fs::remove_file(&link).unwrap();
    let other = write(
        &fixture.0,
        "external/other.toml",
        "include = ['relative.toml']\n",
    );
    symlink(&other, &link).unwrap();
    assert!(binding.verify(control()).is_err());
    fs::remove_file(&link).unwrap();
    let mut command = std::process::Command::new("mkfifo");
    command.arg(&link);
    control().run(&mut command, |_| Ok(())).unwrap();
    assert!(ToolConfiguration::capture(&environment, control()).is_err());
}

#[test]
fn cargo_toml_11_multiline_inline_tables_are_supported() {
    let fixture = temporary_fixture("cargo-include-toml11");
    let environment = environment(&fixture.0);
    write(
        &fixture.0,
        ".cargo/config.toml",
        "include = [{\npath = 'missing.toml',\noptional = true,\n}]\n",
    );
    let mut binding = ToolConfiguration::capture(&environment, control()).unwrap();
    binding.verify(control()).unwrap();
    assert!(binding.verified());
}

#[cfg(unix)]
#[test]
fn cargo_home_alias_of_an_ancestor_is_not_parsed_twice() {
    let fixture = temporary_fixture("cargo-home-alias");
    let root = write(
        &fixture.0,
        ".cargo/config.toml",
        "[env]\nRELATIVE = {value = 'assets', relative = true}\n",
    );
    let home = fixture.0.join("cargo-home");
    std::os::unix::fs::symlink(fixture.0.join(".cargo"), &home).unwrap();
    let environment = environment(&fixture.0);
    let mut binding = ToolConfiguration::capture(&environment, control()).unwrap();
    assert_eq!(binding.cargo_order, [root]);
    binding.verify(control()).unwrap();
    assert!(binding.verified());
}

#[cfg(unix)]
#[test]
fn load_order_agrees_with_real_cargo_compiler_selection() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};
    let fixture = temporary_fixture("cargo-include-real-selection");
    let environment = environment(&fixture.0);
    write(&fixture.0, "Cargo.toml", "[package]\nname = 'config_selection_fixture'\nversion = '0.0.0'\nedition = '2024'\n[workspace]\n");
    write(
        &fixture.0,
        "Cargo.lock",
        "version = 4\n[[package]]\nname = 'config_selection_fixture'\nversion = '0.0.0'\n",
    );
    write(
        &fixture.0,
        "src/lib.rs",
        "// Compiler-selection probe; never compiled.\n",
    );
    let mut programs = Vec::new();
    for label in ["home", "first", "second", "root"] {
        let path = write(
            &fixture.0,
            &format!("tools/{label}"),
            &format!(
                "#!/bin/sh\nprintf '%s\\n' '{label}' > \"$CARGO_SELECTION_OUTPUT\"\nexit 89\n"
            ),
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        programs.push(path);
    }
    let setting = |index: usize| format!("[build]\nrustc = '{}'\n", programs[index].display());
    write(&fixture.0, "cargo-home/config.toml", &setting(0));
    write(&fixture.0, ".cargo/first.toml", &setting(1));
    write(&fixture.0, ".cargo/deep/second.toml", &setting(2));
    write(
        &fixture.0,
        ".cargo/second.toml",
        "include = ['deep/second.toml']\n",
    );
    let cases = [
        (
            "home",
            "include = [{ path = 'missing.toml', optional = true }]\n".to_owned(),
            false,
        ),
        (
            "second",
            "include = ['first.toml', 'second.toml']\n".to_owned(),
            false,
        ),
        (
            "first",
            "include = ['second.toml', 'first.toml']\n".to_owned(),
            false,
        ),
        (
            "root",
            format!("include = ['first.toml', 'second.toml']\n{}", setting(3)),
            false,
        ),
        ("first", "include = ['first.toml']\n".to_owned(), true),
    ];
    for (index, (expected, contents, legacy)) in cases.into_iter().enumerate() {
        if legacy {
            write(&fixture.0, ".cargo/config.toml", "invalid ignored TOML");
            write(&fixture.0, ".cargo/config", &contents);
        } else {
            write(&fixture.0, ".cargo/config.toml", &contents);
        }
        let mut binding = ToolConfiguration::capture(&environment, control()).unwrap();
        let predicted = binding
            .cargo_order
            .iter()
            .rev()
            .find_map(|path| {
                let row = binding.before.iter().find(|row| row.path == *path).unwrap();
                let document: toml::Value =
                    toml::from_str(std::str::from_utf8(&row.contents).unwrap()).unwrap();
                document
                    .get("build")?
                    .get("rustc")?
                    .as_str()
                    .map(PathBuf::from)
            })
            .unwrap();
        assert_eq!(predicted.file_name().unwrap(), expected);
        let output = fixture.0.join(format!("selection-{index}.txt"));
        let mut command = Command::new(crate::cargo_program());
        command
            .current_dir(&fixture.0)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("CARGO_HOME", fixture.0.join("cargo-home"))
            .env("CARGO_SELECTION_OUTPUT", &output)
            .args(["check", "--offline", "--locked"])
            .stdout(Stdio::null())
            .stderr(fs::File::create(fixture.0.join(format!("cargo-{index}.log"))).unwrap());
        let mut launches = 0;
        let result = control().run(&mut command, |_| {
            launches += 1;
            Ok(())
        });
        assert!(
            result.is_err(),
            "the selected probe must stop Cargo before compilation"
        );
        assert_eq!(launches, 1);
        assert_eq!(fs::read_to_string(&output).unwrap().trim(), expected);
        binding.verify(control()).unwrap();
        assert!(binding.verified());
    }
}
