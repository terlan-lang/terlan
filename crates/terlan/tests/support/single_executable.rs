//! Compile with Terlan, deploy one native file, and execute without build inputs.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use terlan_process_owner::{OwnedChild, ProcessControl};

const CLI: &str = r#"module app.Main.
import std.core.Option.{None, Some}.
import std.io.Console.
import std.system.Arguments.
pub main(): Unit -> Console.println(case Arguments.get(0) {
    Some(value) -> value;
    None -> "single executable"
}).
"#;

const SERVICE: &str = r#"module app.Web.
import std.http.{Request, Response, Router}.
import type std.http.{Request, Response, Router}.
pub health(_request: Request): Response -> Response.text("single executable service", 200).
pub router(): Router -> Router.new().get("/health", health).
"#;

fn capture(command: &mut Command) -> String {
    capture_with_timeout(command, Duration::from_secs(15))
}

fn capture_with_timeout(command: &mut Command, timeout: Duration) -> String {
    let output = ProcessControl::new(timeout)
        .capture_stdout(command, 1024 * 1024, |_| Ok(()))
        .unwrap_or_else(|cause| {
            panic!(
                "command exceeded its {}s budget or failed: {cause:?}",
                timeout.as_secs()
            )
        });
    String::from_utf8(output).unwrap()
}

fn build_and_relocate(root: &Path, module: &str, source: &str) -> PathBuf {
    build_and_relocate_with_legacy(root, module, source, false)
}

fn build_and_relocate_with_legacy(
    root: &Path,
    module: &str,
    source: &str,
    legacy: bool,
) -> PathBuf {
    let project = root.join("project");
    let output = root.join("build");
    fs::create_dir_all(project.join("src/app")).unwrap();
    fs::write(project.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n").unwrap();
    fs::write(project.join(format!("src/app/{module}.terl")), source).unwrap();
    if module == "Web" {
        fs::create_dir_all(project.join("assets")).unwrap();
        fs::write(project.join("assets/site.txt"), b"embedded static asset").unwrap();
        let mut manifest = fs::OpenOptions::new()
            .append(true)
            .open(project.join("terlan.toml"))
            .unwrap();
        manifest.write_all(b"\n[web.assets]\ndirectory = \"assets\"\npublic_path = \"/assets\"\ninline_limit = 0\n").unwrap();
    }
    if legacy {
        fs::create_dir_all(output.join("bin")).unwrap();
        fs::create_dir_all(output.join("vm")).unwrap();
        let executable = format!("bin/app{}", if cfg!(windows) { ".cmd" } else { "" });
        let runtime = format!("bin/terlan-vm{}", std::env::consts::EXE_SUFFIX);
        let worker = format!("bin/terlan-native-worker{}", std::env::consts::EXE_SUFFIX);
        for path in [&executable, &runtime, &worker, "vm/app_Main.tvm"] {
            fs::write(output.join(path), b"old bundle member").unwrap();
        }
        fs::write(output.join("keep.txt"), b"unrelated build notes").unwrap();
        fs::write(output.join("terlan-package-build.json"), serde_json::to_vec(&serde_json::json!({
            "schema": "terlan-package-build-v1", "target": "terlan-vm", "artifact": "terlan-vm",
            "package": {"name": "app"}, "executable": {
                "path": executable, "runtime": runtime, "native_worker": worker, "image": "vm/app_Main.tvm"
            }
        })).unwrap()).unwrap();
    }
    // Cold HTTP compilation has a larger source closure than CLI startup.
    // Keep its deadline separate from the deployed application's execution.
    capture_with_timeout(
        Command::new(env!("CARGO_BIN_EXE_terlc"))
            .arg("build")
            .arg(&project)
            .arg("--out-dir")
            .arg(&output)
            .env("TERLAN_VM_RUNNER", env!("CARGO_BIN_EXE_terlan-vm")),
        Duration::from_secs(180),
    );
    assert!(
        !output.join("vm").exists(),
        "application images belong in the compiler cache"
    );
    assert!(
        !output.join("web").exists(),
        "service resources belong in the compiler cache"
    );
    assert_eq!(fs::read_dir(output.join("bin")).unwrap().count(), 1);
    if legacy {
        assert_eq!(
            fs::read(output.join("keep.txt")).unwrap(),
            b"unrelated build notes"
        );
    }
    let name = format!("app{}", std::env::consts::EXE_SUFFIX);
    let deployed = root.join(format!(
        "renamed application{}",
        std::env::consts::EXE_SUFFIX
    ));
    fs::copy(output.join("bin").join(name), &deployed).unwrap();
    fs::remove_dir_all(project).unwrap();
    fs::remove_dir_all(output).unwrap();
    deployed
}

#[test]
fn rebuilding_a_legacy_bundle_removes_only_its_recorded_members() {
    let root = tempfile::tempdir().unwrap();
    let executable = build_and_relocate_with_legacy(root.path(), "Main", CLI, true);
    assert_eq!(
        capture(&mut Command::new(executable)),
        "single executable\n"
    );
}

#[test]
fn copied_application_runs_without_project_runtime_siblings_or_build_tree() {
    let root = tempfile::tempdir().unwrap();
    let executable = build_and_relocate(root.path(), "Main", CLI);
    for argument in ["hello 🌍", "--help", "run", "two words"] {
        let output = capture(
            Command::new(&executable)
                .arg(argument)
                .current_dir(root.path())
                .env("PATH", root.path().join("no-tools"))
                .env_remove("TERLAN_VM_RUNNER")
                .env_remove("TERLAN_NATIVE_WORKER"),
        );
        assert_eq!(output, format!("{argument}\n"));
    }
    assert_eq!(
        capture(
            Command::new(&executable)
                .env("TERLAN_INTERNAL_EXECUTABLE_ROLE", "native-worker")
                .arg("--version")
        ),
        format!("terlan-native-worker {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(
        fs::read_dir(root.path()).unwrap().count(),
        1,
        "startup must not unpack beside the application"
    );
}

#[cfg(target_os = "linux")]
fn sandbox(executable: &Path) -> Command {
    let mut command = Command::new("/usr/bin/bwrap");
    command
        .args([
            "--unshare-user",
            "--unshare-pid",
            "--die-with-parent",
            "--ro-bind",
            "/usr",
            "/usr",
            "--ro-bind",
            "/bin",
            "/bin",
            "--ro-bind",
            "/lib",
            "/lib",
            "--ro-bind-try",
            "/lib64",
            "/lib64",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--dir",
            "/app",
            "--ro-bind",
        ])
        .arg(executable)
        .args([
            "/app/program",
            "--chdir",
            "/tmp",
            "--clearenv",
            "--setenv",
            "PATH",
            "/unavailable",
            "--setenv",
            "LANG",
            "C.UTF-8",
        ]);
    command
}

#[cfg(target_os = "linux")]
#[test]
fn compiled_application_executes_with_compiler_and_stdlib_hidden() {
    let root = tempfile::tempdir().unwrap();
    let executable = build_and_relocate(root.path(), "Main", CLI);
    assert_eq!(
        capture(sandbox(&executable).args(["--unshare-net", "/app/program", "no build inputs"])),
        "no build inputs\n"
    );
}

#[test]
fn compiled_service_serves_after_only_its_executable_is_deployed() {
    let root = tempfile::tempdir().unwrap();
    let executable = build_and_relocate(root.path(), "Web", SERVICE);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let log_path = root.path().join("server.log");
    let log = fs::File::create(&log_path).unwrap();
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = sandbox(&executable);
        command.args([
            "--setenv",
            "PORT",
            &address.port().to_string(),
            "/app/program",
        ]);
        command
    };
    #[cfg(not(target_os = "linux"))]
    let mut command = {
        let mut command = Command::new(&executable);
        command
            .env("PORT", address.port().to_string())
            .env_remove("TERLAN_SERVE_PORT");
        command
    };
    command
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log);
    let mut server = OwnedChild::spawn(command).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut stream = loop {
        assert!(
            server.try_wait().unwrap().is_none(),
            "service exited: {}",
            fs::read_to_string(&log_path).unwrap()
        );
        if let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_millis(100)) {
            break stream;
        }
        assert!(
            Instant::now() < deadline,
            "service failed to start: {}",
            fs::read_to_string(&log_path).unwrap()
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(
        response.ends_with("single executable service"),
        "{response}"
    );
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .write_all(b"GET /assets/site.txt HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut asset = String::new();
    stream.read_to_string(&mut asset).unwrap();
    assert!(asset.starts_with("HTTP/1.1 200"), "{asset}");
    assert!(asset.contains("embedded static asset"), "{asset}");
}
