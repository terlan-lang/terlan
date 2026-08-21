use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

const SOURCE_41: &str = "module cache_probe.\n\npub value(): Int -> 41.\n";
const SOURCE_42: &str = "module cache_probe.\n\npub value(): Int -> 42.\n";

/// Proves the final-link tool is part of native cache identity, while a warm
/// hit with the same linker remains reusable.
#[cfg(unix)]
#[test]
fn native_linker_drift_publishes_a_distinct_verified_cache_entry() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-linker-policy-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let source = root.join("cache_probe.terl");
    let output_dir = root.join("build");
    let first_linker = root.join("first-linker");
    let second_linker = root.join("second-linker");
    fs::create_dir_all(&root).expect("create linker-policy fixture root");
    fs::write(&source, SOURCE_41).expect("write linker-policy fixture source");
    for (path, marker) in [(&first_linker, "first"), (&second_linker, "second")] {
        fs::write(path, format!("#!/bin/sh\n# {marker}\nexec ld \"$@\"\n"))
            .expect("write linker-policy fixture");
        let mut permissions = fs::metadata(path)
            .expect("read linker permissions")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("make fixture linker executable");
    }

    run_build(&root, &source, &output_dir, true, Some(&first_linker));
    run_build(&root, &source, &output_dir, true, Some(&first_linker));
    assert_eq!(
        cache_directories(&output_dir.join(".terlan/native-aot")).len(),
        1
    );
    run_build(&root, &source, &output_dir, true, Some(&second_linker));
    assert_eq!(
        cache_directories(&output_dir.join(".terlan/native-aot")).len(),
        2
    );

    fs::remove_dir_all(root).expect("remove linker-policy fixture root");
}

/// Proves development and release builds cannot reuse each other's native cache.
#[test]
fn native_codegen_policies_publish_and_reuse_distinct_cache_entries() {
    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-policy-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let source = root.join("cache_probe.terl");
    let output_dir = root.join("build");
    fs::create_dir_all(&root).expect("create policy fixture root");
    fs::write(&source, SOURCE_41).expect("write policy fixture source");

    run_build(&root, &source, &output_dir, true, None);
    let image = output_dir.join("vm/cache_probe.tvm");
    let development_image = fs::read(&image).expect("read development image");

    let release = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("--incremental")
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--release")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .output()
        .expect("start release policy build");
    assert!(
        release.status.success(),
        "release policy build failed:\n{}",
        String::from_utf8_lossy(&release.stderr)
    );
    let release_image = fs::read(&image).expect("read release image");
    assert_ne!(release_image, development_image);

    let cache_root = output_dir.join(".terlan/native-aot");
    assert_eq!(cache_directories(&cache_root).len(), 2);

    run_build(
        &root,
        &source,
        &output_dir,
        true,
        Some(&root.join("linker-must-not-run")),
    );
    assert_eq!(
        fs::read(&image).expect("read restored development image"),
        development_image
    );

    fs::remove_dir_all(&root).expect("remove policy fixture root");
}

#[test]
fn native_aot_cache_verifies_and_recovers_every_required_file() {
    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-cache-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let source = root.join("cache_probe.terl");
    let output_dir = root.join("build");
    fs::create_dir_all(&root).expect("create cache fixture root");
    fs::write(&source, SOURCE_41).expect("write cache fixture source");

    run_build(&root, &source, &output_dir, false, None);
    let image_name = "cache_probe.tvm";
    let deployed_image = output_dir.join("vm").join(image_name);
    let original_image_sha = Sha256::digest(fs::read(&deployed_image).expect("read native image"));
    let cache_root = output_dir.join(".terlan/native-aot");
    let original_cache = only_cache_directory(&cache_root);
    let files = cache_files(&original_cache, &image_name);
    let manifest = fs::read_to_string(&files.manifest).expect("read native cache manifest");
    assert!(manifest.starts_with("terlan-native-cache-v1\n"));
    assert_eq!(
        manifest
            .lines()
            .filter(|line| line.starts_with("file "))
            .count(),
        3
    );

    run_build(
        &root,
        &source,
        &output_dir,
        true,
        Some(&root.join("linker-must-not-run")),
    );

    fs::write(&files.object, b"poisoned object").expect("poison native object");
    run_build(&root, &source, &output_dir, true, None);
    assert_ne!(fs::read(&files.object).unwrap(), b"poisoned object");

    fs::remove_file(&files.descriptor).expect("remove descriptor object");
    run_build(&root, &source, &output_dir, true, None);
    assert!(files.descriptor.is_file());

    fs::write(&files.manifest, b"poisoned manifest").expect("poison manifest");
    run_build(&root, &source, &output_dir, true, None);
    assert!(fs::read(&files.manifest)
        .unwrap()
        .starts_with(b"terlan-native-cache-v1\n"));

    fs::write(&files.image, b"poisoned image").expect("poison cached image");
    run_build(&root, &source, &output_dir, true, None);
    assert_ne!(fs::read(&files.image).unwrap(), b"poisoned image");

    fs::write(&deployed_image, b"poisoned deployment").expect("poison deployed image");
    run_build(
        &root,
        &source,
        &output_dir,
        true,
        Some(&root.join("linker-must-not-run")),
    );
    assert_eq!(
        fs::read(&deployed_image).unwrap(),
        fs::read(&files.image).unwrap()
    );

    fs::write(&source, SOURCE_42).expect("write alternate cache input");
    run_build(&root, &source, &output_dir, true, None);
    let variant_cache = cache_directories(&cache_root)
        .into_iter()
        .find(|path| path != &original_cache)
        .expect("variant cache directory");
    let variant_files = cache_files(&variant_cache, &image_name);
    let variant_image = fs::read(&variant_files.image).expect("read variant image");

    fs::write(&source, SOURCE_41).expect("restore original cache input");
    run_build(&root, &source, &output_dir, true, None);
    let reuse_stamp = only_reuse_stamp(&cache_root);
    let original_input = original_cache
        .file_name()
        .and_then(|name| name.to_str())
        .expect("original cache identity");
    let variant_input = variant_cache
        .file_name()
        .and_then(|name| name.to_str())
        .expect("variant cache identity");
    let stamp = fs::read_to_string(&reuse_stamp).expect("read native reuse stamp");
    assert!(stamp.contains(&format!("input-sha256 {original_input}\n")));
    fs::write(
        &reuse_stamp,
        stamp.replacen(
            &format!("input-sha256 {original_input}\n"),
            &format!("input-sha256 {variant_input}\n"),
            1,
        ),
    )
    .expect("poison reuse cache key");
    fs::write(&deployed_image, &variant_image).expect("deploy wrong valid generation");
    run_build(
        &root,
        &source,
        &output_dir,
        true,
        Some(&root.join("linker-must-not-run")),
    );
    assert_eq!(
        Sha256::digest(fs::read(&deployed_image).expect("read source-bound image")),
        original_image_sha
    );
    assert!(fs::read_to_string(&reuse_stamp)
        .expect("read repaired native reuse stamp")
        .contains(&format!("input-sha256 {original_input}\n")));

    fs::write(&files.image, &variant_image).expect("place valid image under wrong cache key");
    rewrite_manifest_file_record(&files.manifest, &image_name, &variant_image);
    run_build(&root, &source, &output_dir, true, None);
    assert_eq!(
        Sha256::digest(fs::read(&deployed_image).expect("read recovered native image")),
        original_image_sha
    );
    assert_ne!(fs::read(&files.image).unwrap(), variant_image);

    fs::remove_dir_all(&root).expect("remove native cache fixture root");
}

#[cfg(unix)]
#[test]
fn concurrent_native_aot_builds_publish_one_verified_cache_entry() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-cache-concurrent-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let source = root.join("cache_probe.terl");
    let shared_cache = root.join("shared-cache");
    let linker = root.join("counting-linker");
    let link_log = root.join("link.log");
    fs::create_dir_all(&root).expect("create concurrent cache fixture root");
    fs::write(&source, SOURCE_41).expect("write concurrent cache fixture source");
    fs::write(
        &linker,
        "#!/bin/sh\nprintf 'link\\n' >> \"$TERLAN_TEST_LINK_LOG\"\nsleep 1\nexec cc \"$@\"\n",
    )
    .expect("write counting linker");
    let mut permissions = fs::metadata(&linker)
        .expect("read counting linker metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&linker, permissions).expect("make counting linker executable");

    let spawn = |output_dir: &Path| {
        Command::new(env!("CARGO_BIN_EXE_terlc"))
            .arg("--incremental")
            .arg("--cache-dir")
            .arg(&shared_cache)
            .arg("build")
            .arg(&source)
            .arg("--target")
            .arg("terlan-vm")
            .arg("--out-dir")
            .arg(output_dir)
            .env("RUSTC", root.join("rustc-must-not-run"))
            .env("TERLAN_NATIVE_LINKER", &linker)
            .env("TERLAN_TEST_LINK_LOG", &link_log)
            .spawn()
            .expect("start concurrent native cache build")
    };
    let first = spawn(&root.join("build-one"));
    let second = spawn(&root.join("build-two"));
    for (label, child) in [("first", first), ("second", second)] {
        let output = child
            .wait_with_output()
            .unwrap_or_else(|error| panic!("wait for {label} concurrent build: {error}"));
        assert!(
            output.status.success(),
            "{label} concurrent build failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert_eq!(
        fs::read_to_string(&link_log)
            .expect("read linker invocation log")
            .lines()
            .count(),
        1
    );
    let cache_root = shared_cache.join("native-aot");
    let cache = only_cache_directory(&cache_root);
    assert!(cache.join("build.lock").is_file());
    assert!(fs::read_dir(&cache)
        .expect("read concurrent cache files")
        .all(|entry| !entry
            .expect("read concurrent cache file")
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    let image_name = "cache_probe.tvm";
    let files = cache_files(&cache, image_name);
    assert_eq!(
        fs::read(&root.join("build-one/vm").join(image_name)).unwrap(),
        fs::read(&files.image).unwrap()
    );
    assert_eq!(
        fs::read(&root.join("build-two/vm").join(image_name)).unwrap(),
        fs::read(&files.image).unwrap()
    );

    fs::remove_dir_all(&root).expect("remove concurrent cache fixture root");
}

#[cfg(unix)]
#[test]
fn killed_native_aot_builder_releases_cache_ownership() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-cache-killed-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let source = root.join("cache_probe.terl");
    let shared_cache = root.join("shared-cache");
    let linker = root.join("blocking-linker");
    let ready = root.join("linker.ready");
    fs::create_dir_all(&root).expect("create killed-builder fixture root");
    fs::write(&source, SOURCE_41).expect("write killed-builder source");
    fs::write(
        &linker,
        "#!/bin/sh\nprintf ready > \"$TERLAN_TEST_LINK_READY\"\nsleep 1\nexec cc \"$@\"\n",
    )
    .expect("write blocking linker");
    let mut permissions = fs::metadata(&linker)
        .expect("read blocking linker metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&linker, permissions).expect("make blocking linker executable");

    let mut interrupted = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("--incremental")
        .arg("--cache-dir")
        .arg(&shared_cache)
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(root.join("interrupted-build"))
        .env("RUSTC", root.join("rustc-must-not-run"))
        .env("TERLAN_NATIVE_LINKER", &linker)
        .env("TERLAN_TEST_LINK_READY", &ready)
        .spawn()
        .expect("start interrupted native cache build");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(ready.exists(), "blocking linker did not start");
    interrupted.kill().expect("kill native cache builder");
    interrupted
        .wait()
        .expect("reap killed native cache builder");

    let recovery_output = root.join("recovered-build");
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("--incremental")
        .arg("--cache-dir")
        .arg(&shared_cache)
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&recovery_output)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .output()
        .expect("start cache recovery build");
    assert!(
        output.status.success(),
        "cache recovery build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(recovery_output.join("vm/cache_probe.tvm").is_file());

    std::thread::sleep(Duration::from_secs(1));
    fs::remove_dir_all(&root).expect("remove killed-builder fixture root");
}

#[test]
fn vm_aot_timings_report_compile_and_native_artifact_phases() {
    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-timings-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let source = root.join("cache_probe.terl");
    let output_dir = root.join("build");
    fs::create_dir_all(&root).expect("create timing fixture root");
    fs::write(&source, SOURCE_41).expect("write timing fixture source");

    let output = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("--incremental")
        .arg("--timings")
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .output()
        .expect("start timed native build");
    assert!(
        output.status.success(),
        "timed native build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("terlc timing: vm.compile:"), "{stderr}");
    assert!(
        stderr.contains("terlc timing: vm.aot-and-artifact:"),
        "{stderr}"
    );
    assert_eq!(stderr.matches("terlc timing:").count(), 2, "{stderr}");

    fs::remove_dir_all(&root).expect("remove timing fixture root");
}

#[test]
fn vm_aot_warm_noop_p95_stays_under_one_second() {
    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-warm-budget-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let source = root.join("cache_probe.terl");
    let output_dir = root.join("build");
    fs::create_dir_all(&root).expect("create warm-budget fixture root");
    fs::write(&source, SOURCE_41).expect("write warm-budget fixture source");
    run_build(&root, &source, &output_dir, true, None);

    let invalid_linker = root.join("linker-must-not-run");
    let mut samples = Vec::with_capacity(7);
    for _ in 0..7 {
        let started = Instant::now();
        run_build(&root, &source, &output_dir, true, Some(&invalid_linker));
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    let p95 = samples[samples.len() - 1];
    assert!(
        p95 < Duration::from_secs(1),
        "warm native no-op p95 {p95:?} exceeded 1s; samples={samples:?}"
    );

    fs::remove_dir_all(&root).expect("remove warm-budget fixture root");
}

#[cfg(unix)]
#[test]
fn unchanged_repl_generation_reuses_native_image_without_relinking() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;

    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-repl-reuse-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let linker = root.join("counting-linker");
    let link_log = root.join("link.log");
    fs::create_dir_all(&root).expect("create REPL reuse fixture root");
    fs::write(
        &linker,
        "#!/bin/sh\nprintf 'link\\n' >> \"$TERLAN_TEST_LINK_LOG\"\nexec ld \"$@\"\n",
    )
    .expect("write counting linker");
    let mut permissions = fs::metadata(&linker)
        .expect("read counting linker metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&linker, permissions).expect("make counting linker executable");

    let mut repl = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("repl")
        .env("RUSTC", root.join("rustc-must-not-run"))
        .env("TERLAN_NATIVE_LINKER", &linker)
        .env("TERLAN_TEST_LINK_LOG", &link_log)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start AOT REPL reuse fixture");
    repl.stdin
        .as_mut()
        .expect("REPL stdin")
        .write_all(b"1 + 2.\n1 + 2.\n:quit\n")
        .expect("write repeated REPL expression");
    drop(repl.stdin.take());
    let output = repl
        .wait_with_output()
        .expect("wait for AOT REPL reuse fixture");
    assert!(
        output.status.success(),
        "AOT REPL reuse fixture failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout)
            .matches("repl> 3\n")
            .count(),
        2,
        "both unchanged generations must execute"
    );
    assert_eq!(
        fs::read_to_string(&link_log)
            .expect("read REPL linker log")
            .lines()
            .count(),
        1,
        "the unchanged REPL generation must reuse the first native image"
    );

    fs::remove_dir_all(&root).expect("remove REPL reuse fixture root");
}

struct CacheFiles {
    manifest: PathBuf,
    object: PathBuf,
    descriptor: PathBuf,
    image: PathBuf,
}

fn cache_files(directory: &Path, image_name: &str) -> CacheFiles {
    let entries = fs::read_dir(directory)
        .expect("read native cache directory")
        .map(|entry| entry.expect("read native cache entry").path())
        .collect::<Vec<_>>();
    let find = |needle: &str| {
        entries
            .iter()
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.contains(needle))
            })
            .cloned()
            .unwrap_or_else(|| panic!("missing native cache file containing {needle}"))
    };
    CacheFiles {
        manifest: directory.join("manifest.v1"),
        object: find(".native."),
        descriptor: find(".descriptor."),
        image: directory.join(image_name),
    }
}

fn cache_directories(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .expect("read native cache root")
        .map(|entry| entry.expect("read native cache directory").path())
        .filter(|path| path.is_dir())
        .collect()
}

fn only_cache_directory(root: &Path) -> PathBuf {
    let directories = cache_directories(root);
    assert_eq!(directories.len(), 1);
    directories[0].clone()
}

fn only_reuse_stamp(root: &Path) -> PathBuf {
    let stamps = fs::read_dir(root)
        .expect("read native cache root")
        .map(|entry| entry.expect("read native cache entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("reuse-") && name.ends_with(".stamp"))
        })
        .collect::<Vec<_>>();
    assert_eq!(stamps.len(), 1);
    stamps[0].clone()
}

fn run_build(
    root: &Path,
    source: &Path,
    output_dir: &Path,
    incremental: bool,
    linker: Option<&Path>,
) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_terlc"));
    if incremental {
        command.arg("--incremental");
    }
    command
        .arg("build")
        .arg(source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"));
    if let Some(linker) = linker {
        if linker.file_name().and_then(|name| name.to_str()) == Some("linker-must-not-run") {
            command.env("TERLAN_NATIVE_CACHE_MISS_POLICY", "error");
        } else {
            command.env("TERLAN_NATIVE_LINKER", linker);
        }
    }
    let output = command.output().expect("start native cache build");
    assert!(
        output.status.success(),
        "native cache build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn rewrite_manifest_file_record(path: &Path, file_name: &str, bytes: &[u8]) {
    let digest = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let replacement = format!("file {file_name} {} {digest}", bytes.len());
    let manifest = fs::read_to_string(path).expect("read cache manifest for rewrite");
    let rewritten = manifest
        .lines()
        .map(|line| {
            if line.starts_with(&format!("file {file_name} ")) {
                replacement.as_str()
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(path, rewritten).expect("rewrite cache manifest record");
}
