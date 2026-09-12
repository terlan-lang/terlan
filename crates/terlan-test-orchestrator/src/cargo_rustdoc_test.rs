//! Real Cargo differential for an observation hook which does not replace doctest runtools.

use super::{compile_units, temporary_fixture};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use terlan_process_owner::ProcessControl;

const WRAPPER: &str = r#"
fn main() -> std::process::ExitCode {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let records = std::path::PathBuf::from(std::env::var_os("DOC_HOOK_RECORDS").unwrap());
    let record = format!("cwd={:?}\nrustdoc={:?}\nargs={:?}\n", std::env::current_dir().unwrap(), std::env::var_os("RUSTDOC"), args);
    std::fs::write(records.join(format!("{}.txt", std::process::id())), record).unwrap();
    let status = std::process::Command::new(std::env::var_os("DOC_REAL_RUSTDOC").unwrap()).args(args).status().unwrap();
    std::process::ExitCode::from(status.code().and_then(|code| u8::try_from(code).ok()).unwrap_or(1))
}
"#;

const LIBRARY: &str = r#"
//! ```
//! assert_eq!(doc_hook::count(), 0);
//! assert_eq!(doc_hook::rustdoc(), option_env!("RUSTDOC"));
//! assert_eq!(doc_hook::rustdoc(), std::env::var("RUSTDOC").ok().as_deref());
//! doc_hook::record("one");
//! ```
//!
//! ```
//! assert_eq!(doc_hook::count(), 0);
//! assert_eq!(doc_hook::build_rustdoc(), option_env!("CARGO_BUILD_RUSTDOC"));
//! assert_eq!(doc_hook::build_rustdoc(), std::env::var("CARGO_BUILD_RUSTDOC").ok().as_deref());
//! doc_hook::record("two");
//! ```
pub fn count() -> usize {
    static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}
pub fn rustdoc() -> Option<&'static str> { option_env!("RUSTDOC") }
pub fn build_rustdoc() -> Option<&'static str> { option_env!("CARGO_BUILD_RUSTDOC") }
pub fn record(label: &str) {
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(std::env::var_os("DOC_BODY_RECORDS").unwrap()).unwrap();
    std::io::Write::write_all(&mut file, format!("{label}\n").as_bytes()).unwrap();
}
"#;

fn command(root: &Path, cycle: &Path, rustdoc: &Path, mode: &str) -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .env_remove("RUSTDOC")
        .env_remove("CARGO_BUILD_RUSTDOC")
        .env("CARGO_TARGET_DIR", cycle.join("target"))
        .env("DOC_REAL_RUSTDOC", rustdoc)
        .env("DOC_HOOK_RECORDS", cycle.join("hooks"))
        .env("DOC_BODY_RECORDS", cycle.join("bodies"))
        .env("DOC_BUILD_RECORDS", cycle.join("builds"));
    match mode {
        "default" => (),
        "config-env" => {
            command.env("CARGO_BUILD_RUSTDOC", rustdoc);
        }
        "direct-env" | "forced-child-env" | "string-child-env" | "relative-child-env" => {
            command.env("RUSTDOC", rustdoc);
        }
        _ => panic!("unknown fixture mode"),
    }
    command
}

fn cycle(
    root: &Path,
    rustdoc: &Path,
    wrapper: &Path,
    mode: &str,
    observed: bool,
) -> (Vec<String>, u128) {
    let cycle = root.join(format!("{mode}-{observed}"));
    fs::create_dir(&cycle).unwrap();
    fs::create_dir(cycle.join("hooks")).unwrap();
    let control = ProcessControl::new(Duration::from_secs(30));
    let started = Instant::now();
    let mut output = control
        .capture_stdout(
            command(root, &cycle, rustdoc, mode).args([
                "build",
                "--locked",
                "--lib",
                "--message-format=json",
            ]),
            1024 * 1024,
            |_| Ok(()),
        )
        .unwrap();
    let mut docs = command(root, &cycle, rustdoc, mode);
    docs.args(["test", "--locked", "--doc", "--message-format=json"]);
    if observed {
        let config_path = root.join(".cargo/config.toml");
        let config: toml::Value =
            toml::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        let settings =
            crate::cargo_tool_settings::CargoToolSettings::from_document(&config_path, &config)
                .unwrap();
        let mut entries = std::env::vars_os().collect::<std::collections::BTreeMap<_, _>>();
        for (key, value) in docs.get_envs() {
            match value {
                Some(value) => {
                    entries.insert(key.to_owned(), value.to_owned());
                }
                None => {
                    entries.remove(key);
                }
            }
        }
        let environment =
            crate::execution_environment::ExecutionEnvironment::from_entries(entries, root)
                .unwrap();
        settings
            .observe_rustdoc(&mut docs, &environment, wrapper)
            .unwrap();
    }
    docs.args([
        "--",
        "--test-threads",
        "1",
        "--format",
        "pretty",
        "--color",
        "never",
    ]);
    let doc_output = control
        .capture_stdout(&mut docs, 1024 * 1024, |_| Ok(()))
        .unwrap();
    let elapsed = started.elapsed().as_millis();
    let text = String::from_utf8(doc_output.clone()).unwrap();
    let pretty = text
        .lines()
        .filter(|line| !line.starts_with('{'))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(
        crate::doctest_output::inspect(pretty.as_bytes())
            .unwrap()
            .json()["passed"],
        2
    );
    output.extend(doc_output);
    let mut bodies = fs::read_to_string(cycle.join("bodies"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    bodies.sort();
    assert_eq!(bodies, ["one", "two"]);
    assert_eq!(
        fs::read_to_string(cycle.join("builds")).unwrap(),
        "build\n",
        "build script reran in {mode}/{observed}"
    );
    let hooks = fs::read_dir(cycle.join("hooks"))
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(hooks.len(), usize::from(observed), "{mode}: {hooks:?}");
    if observed {
        assert!(hooks[0].contains("\"--test\""));
        assert!(!hooks[0].contains("\"--test-runtool\""));
    }
    (compile_units(&output), elapsed)
}

#[test]
fn rustdoc_configuration_hook_preserves_child_environment_and_compiler_units() {
    let fixture = temporary_fixture("cargo-rustdoc-hook");
    let root = &fixture.0;
    let control = ProcessControl::new(Duration::from_secs(30));
    let rustdoc = control
        .capture_stdout(
            Command::new("rustup").args(["which", "rustdoc"]),
            16 * 1024,
            |_| Ok(()),
        )
        .unwrap();
    let rustdoc = std::path::PathBuf::from(std::str::from_utf8(&rustdoc).unwrap().trim());
    let wrapper = root.join(format!("rustdoc-hook{}", std::env::consts::EXE_SUFFIX));
    fs::write(root.join("hook.rs"), WRAPPER).unwrap();
    control
        .run(
            Command::new("rustc")
                .arg("--edition=2021")
                .arg(root.join("hook.rs"))
                .arg("-o")
                .arg(&wrapper),
            |_| Ok(()),
        )
        .unwrap();
    fs::write(root.join("Cargo.toml"), "[package]\nname=\"doc_hook\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[lib]\npath=\"lib.rs\"\n").unwrap();
    fs::write(
        root.join("Cargo.lock"),
        "version=4\n[[package]]\nname=\"doc_hook\"\nversion=\"0.0.0\"\n",
    )
    .unwrap();
    fs::write(root.join("lib.rs"), LIBRARY).unwrap();
    fs::write(root.join("build.rs"), r#"
fn main() {
    println!("cargo:rerun-if-env-changed=RUSTDOC");
    println!("cargo:rerun-if-env-changed=CARGO_BUILD_RUSTDOC");
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(std::env::var_os("DOC_BUILD_RECORDS").unwrap()).unwrap();
    std::io::Write::write_all(&mut file, b"build\n").unwrap();
}
"#).unwrap();
    fs::create_dir(root.join(".cargo")).unwrap();
    for mode in [
        "default",
        "config-env",
        "direct-env",
        "forced-child-env",
        "string-child-env",
        "relative-child-env",
    ] {
        fs::write(
            root.join(".cargo/config.toml"),
            match mode {
                "forced-child-env" => "[env]\nRUSTDOC={value=\"configured-child-value\",force=true}\n",
                "string-child-env" => "[env]\nRUSTDOC=\"configured-child-value\"\n",
                "relative-child-env" => "[env]\nRUSTDOC={value=\"configured-relative-value\",force=true,relative=true}\n",
                _ => "",
            },
        )
        .unwrap();
        let baseline = cycle(root, &rustdoc, &wrapper, mode, false);
        let observed = cycle(root, &rustdoc, &wrapper, mode, true);
        assert_eq!(baseline.0, observed.0, "compiler units changed in {mode}");
        eprintln!("Rustdoc hook {mode}: baseline {}ms, observed {}ms; {} identical compiler units, build script once, two isolated bodies once", baseline.1, observed.1, baseline.0.len());
    }
}
