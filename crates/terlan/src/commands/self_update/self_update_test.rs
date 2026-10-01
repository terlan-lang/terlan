use super::*;
use serde_json::{json, Value};

const ARTIFACT: &str = "terlc-linux-x86_64.tar.gz";

fn release(tag: &str) -> Value {
    json!({"tag_name": tag, "draft": false, "prerelease": false,
        "assets": [{"name": ARTIFACT}, {"name": format!("{ARTIFACT}.sha256")}]})
}

#[test]
fn arguments_default_to_latest_and_reject_ambiguous_or_unsafe_input() {
    for args in [vec![], vec!["latest"], vec!["--version", "latest"]] {
        assert_eq!(
            parse_args(&args.into_iter().map(String::from).collect::<Vec<_>>()).unwrap(),
            Options::Latest
        );
    }
    for args in [vec!["0.0.9"], vec!["v0.0.9"], vec!["--version", "0.0.9"]] {
        assert_eq!(
            parse_args(&args.into_iter().map(String::from).collect::<Vec<_>>()).unwrap(),
            Options::Version("v0.0.9".into())
        );
    }
    for args in [
        vec!["--version"],
        vec!["--list", "0.0.9"],
        vec!["--list", "--interactive"],
        vec!["--unknown"],
        vec!["../../bad"],
        vec!["0.0.9;echo bad"],
        vec!["0.0.9+meta"],
    ] {
        let args = args.into_iter().map(String::from).collect::<Vec<_>>();
        assert!(parse_args(&args).is_err(), "{args:?}");
        assert_eq!(run(&args), ExitCode::from(2));
    }
}

#[test]
fn selection_supports_numbers_versions_default_cancellation_and_retry() {
    let available: Vec<releases::Release> =
        serde_json::from_value(json!([release("v0.0.10"), release("v0.0.9")])).unwrap();
    for (input, expected) in [
        ("\n", Some("v0.0.9")),
        ("1\n", Some("v0.0.10")),
        ("0.0.10\n", Some("v0.0.10")),
        ("v0.0.9\n", Some("v0.0.9")),
        ("0\n999\ninvalid\n2\n", Some("v0.0.9")),
        ("q\n", None),
        ("", None),
    ] {
        let result = prompt_selection(
            &available,
            Some(&available[1]),
            &mut input.as_bytes(),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            result.as_ref().map(|release| release.tag_name.as_str()),
            expected
        );
    }
    let result =
        prompt_selection(&available, None, &mut "\n1\n".as_bytes(), &mut Vec::new()).unwrap();
    assert_eq!(result.unwrap().tag_name, "v0.0.10");
}

#[test]
fn platform_mapping_and_installation_layouts() {
    for os in ["linux", "macos", "windows"] {
        for arch in ["x86_64", "aarch64"] {
            let name = releases::platform_artifact(os, arch).unwrap();
            assert!(name.contains(arch));
            assert_eq!(name.ends_with(".zip"), os == "windows");
        }
    }
    assert!(releases::platform_artifact("freebsd", "x86_64").is_err());
    assert!(releases::platform_artifact("linux", "x86").is_err());
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin with spaces");
    std::fs::create_dir_all(&bin).unwrap();
    assert_eq!(
        install::destinations(&bin.join("terlc")).unwrap(),
        (bin.clone(), root.path().join("share/terlan"))
    );
    std::fs::create_dir_all(bin.join("share/terlan")).unwrap();
    assert_eq!(
        install::destinations(&bin.join("terlc")).unwrap().1,
        bin.join("share/terlan")
    );
}

#[cfg(feature = "registry-network")]
mod github {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{Duration, Instant};

    /// Serves expected GitHub routes locally, with bounded waits on test failures.
    fn server(responses: Vec<(String, u16, String)>) -> (releases::Client, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}/releases", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            for (route, status, body) in responses {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(error) => panic!("missing request for {route}: {error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert!(
                    line.starts_with(&format!("GET /releases{route} HTTP/1.1")),
                    "{line}"
                );
                let mut headers = String::new();
                loop {
                    line.clear();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    headers.push_str(&line.to_lowercase());
                }
                assert!(headers.contains("user-agent: terlc/"));
                assert!(headers.contains("accept: application/vnd.github+json"));
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (releases::Client::for_test(base), worker)
    }

    #[test]
    fn listing_paginates_sorts_versions_and_filters_uninstallable_releases() {
        let mut first = vec![release("v0.0.9"); 100];
        first[0] = release("v0.0.10");
        first[1] = release("v9.0.0");
        first[1]["draft"] = json!(true);
        first[2]["tag_name"] = json!("not-a-version");
        first[3] = release("v8.0.0");
        first[3]["assets"] = json!([]);
        first[4] = release("v7.0.0");
        first[4]["assets"] = json!([{"name": ARTIFACT}]);
        first[5] = release("v0.1.0-rc.1");
        let (client, worker) = server(vec![
            ("?per_page=100&page=1".into(), 200, json!(first).to_string()),
            (
                "?per_page=100&page=2".into(),
                200,
                json!([release("v0.0.8")]).to_string(),
            ),
        ]);
        let releases = client.list(ARTIFACT).unwrap();
        assert_eq!(
            releases
                .iter()
                .map(|release| release.tag_name.as_str())
                .collect::<Vec<_>>(),
            ["v0.1.0-rc.1", "v0.0.10", "v0.0.9", "v0.0.8"]
        );
        assert!(releases[0].is_prerelease());
        worker.join().unwrap();
    }

    #[test]
    fn latest_uses_github_designation_and_explicit_versions_allow_prereleases() {
        let (client, worker) = server(vec![
            ("/latest".into(), 200, release("v0.0.8").to_string()),
            (
                "/tags/v0.1.0-rc.1".into(),
                200,
                release("v0.1.0-rc.1").to_string(),
            ),
        ]);
        assert_eq!(client.latest(ARTIFACT).unwrap().tag_name, "v0.0.8");
        assert!(client
            .by_tag("v0.1.0-rc.1", ARTIFACT)
            .unwrap()
            .is_prerelease());
        worker.join().unwrap();
    }

    #[test]
    fn rejects_missing_releases_rate_limits_invalid_json_and_incompatible_latest() {
        for (status, body, expected) in [
            (404, "{}".into(), "not found"),
            (403, "{}".into(), "rate limit"),
            (429, "{}".into(), "rate limit"),
            (500, "{}".into(), "HTTP 500"),
            (200, "not json".into(), "invalid GitHub"),
            (200, release("v0.1.0-rc.1").to_string(), "latest stable"),
        ] {
            let (client, worker) = server(vec![("/latest".into(), status, body)]);
            assert!(client.latest(ARTIFACT).unwrap_err().contains(expected));
            worker.join().unwrap();
        }
        let (client, worker) = server(vec![("?per_page=100&page=1".into(), 200, "[]".into())]);
        assert!(client
            .list(ARTIFACT)
            .unwrap_err()
            .contains("no published releases"));
        worker.join().unwrap();
        let (client, worker) = server(vec![(
            "/tags/v0.0.9".into(),
            200,
            release("v0.0.8").to_string(),
        )]);
        assert!(client.by_tag("v0.0.9", ARTIFACT).is_err());
        worker.join().unwrap();
    }
}

#[cfg(unix)]
mod installer {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;
    use std::path::Path;
    use std::process::Command;

    /// Runs only in a copied test executable, then replaces it with the installer.
    #[test]
    fn installer_handoff_child() {
        let Some(root) = std::env::var_os("TERLAN_SELF_UPDATE_TEST_ROOT") else {
            return;
        };
        let root = Path::new(&root);
        let executable = std::env::current_exe().unwrap();
        let (bin, share) = install::destinations(&executable).unwrap();
        let mut command = install::unix_command("v0.0.8", &bin, &share);
        command.env(
            "TERLAN_RELEASE_BASE_URL",
            format!("file://{}", root.join("releases").display()),
        );
        panic!("installer exec failed: {}", command.exec());
    }

    fn write_executable(path: &Path, contents: &str) {
        fs::write(path, contents).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn checksum_row(root: &Path, relative: &str) -> String {
        let digest: String = Sha256::digest(fs::read(root.join(relative)).unwrap())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        format!("{digest}  {relative}\n")
    }

    /// Builds a complete local bundle with real external and internal checksums.
    fn fixture(root: &Path, fail_validation: bool) -> std::path::PathBuf {
        let payload = root.join("payload");
        for directory in ["std", "editors/vscode", "tree-sitter-terlan", "runtime"] {
            fs::create_dir_all(payload.join("share/terlan").join(directory)).unwrap();
        }
        fs::write(
            payload.join("share/terlan/runtime/release-self-test.tvm"),
            "fixture",
        )
        .unwrap();
        for name in ["terlc", "terlan-vm", "terlan-native-worker", "terlan-lsp"] {
            let script = if name == "terlan-vm" && fail_validation {
                "#!/bin/sh\nif [ \"$1\" = validate-package ]; then exit 19; fi\necho 'terlan-vm 0.0.8'\n".to_string()
            } else {
                format!("#!/bin/sh\necho '{name} 0.0.8'\n")
            };
            write_executable(&payload.join(name), &script);
        }
        for name in ["terlan-install-manifest.json", "terlan-release.json"] {
            fs::write(payload.join(name), "{}").unwrap();
        }
        let hashes: String = [
            "terlc",
            "terlan-vm",
            "terlan-native-worker",
            "terlan-lsp",
            "share/terlan/runtime/release-self-test.tvm",
            "terlan-install-manifest.json",
            "terlan-release.json",
        ]
        .iter()
        .map(|relative| checksum_row(&payload, relative))
        .collect();
        fs::write(payload.join("SHA256SUMS"), hashes).unwrap();
        let releases = root.join("releases/v0.0.8");
        fs::create_dir_all(&releases).unwrap();
        let artifact =
            releases::platform_artifact(std::env::consts::OS, std::env::consts::ARCH).unwrap();
        let archive = releases.join(&artifact);
        assert!(Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&payload)
            .arg(".")
            .status()
            .unwrap()
            .success());
        fs::write(
            releases.join(format!("{artifact}.sha256")),
            checksum_row(&releases, &artifact),
        )
        .unwrap();
        archive
    }

    #[test]
    fn updates_running_executable_and_rolls_back_validation_failure() {
        for (fail_validation, corrupt_checksum) in [(false, false), (true, false), (false, true)] {
            let root = tempfile::tempdir().unwrap();
            let bin = root.path().join("prefix with spaces/bin");
            let share = bin.parent().unwrap().join("share/terlan");
            fs::create_dir_all(&bin).unwrap();
            fs::create_dir_all(&share).unwrap();
            fs::write(share.join("original"), "preserve me").unwrap();
            fs::copy(std::env::current_exe().unwrap(), bin.join("terlc")).unwrap();
            write_executable(&bin.join("terlan-vm"), "#!/bin/sh\necho old-vm\n");
            let before = fs::metadata(bin.join("terlc")).unwrap().len();
            let archive = fixture(root.path(), fail_validation);
            if corrupt_checksum {
                fs::write(&archive, "corrupt archive").unwrap();
            }
            let output = Command::new(bin.join("terlc"))
                .args(["installer_handoff_child", "--nocapture"])
                .env("TERLAN_SELF_UPDATE_TEST_ROOT", root.path())
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                !fail_validation && !corrupt_checksum,
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if fail_validation || corrupt_checksum {
                assert_eq!(fs::metadata(bin.join("terlc")).unwrap().len(), before);
                assert_eq!(
                    fs::read_to_string(share.join("original")).unwrap(),
                    "preserve me"
                );
                assert!(fs::read_to_string(bin.join("terlan-vm"))
                    .unwrap()
                    .contains("old-vm"));
                assert!(!bin.join("terlan-native-worker").exists());
            } else {
                let version = Command::new(bin.join("terlc"))
                    .arg("--version")
                    .output()
                    .unwrap();
                assert_eq!(
                    String::from_utf8_lossy(&version.stdout).trim(),
                    "terlc 0.0.8"
                );
                assert!(share.join("terlan-release.json").is_file());
                assert!(bin.join("terlan-native-worker").is_file());
            }
        }
    }
}
