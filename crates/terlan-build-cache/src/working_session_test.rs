//! Abandoned-writer recovery must not adopt active, recent or foreign payloads.
use super::*;
use std::process::{Child, Command, Stdio};
use std::time::Instant;

/// Always reap an owned fixture compiler, including assertion and timeout paths.
struct Compiler(Child);

impl Drop for Compiler {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_compiler(child: &mut Compiler) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "fixture compiler failed: {status}");
            return;
        }
        assert!(Instant::now() < deadline, "fixture compiler exceeded 30s");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn partial_and_empty_working_sessions_are_audited_then_retired() {
    for partial in [false, true] {
        let fixture = Fixture::new();
        let working = fixture.session("s-2-1-working");
        let latest = fixture.session("s-1-1-aaa");
        fs::remove_file(working.join("dep-graph.bin")).unwrap();
        fs::remove_file(working.join("abc.o")).unwrap();
        if partial {
            fs::write(working.join("dep-graph.part.bin"), b"RSIC").unwrap();
        }
        let audit = fixture.run(false).unwrap();
        assert_eq!(audit.abandoned_sessions, 1);
        assert_eq!(audit.redundant_sessions, 0);
        assert_eq!(audit.removed_sessions, 0);
        assert!(working.exists() && latest.exists());
        let pruned = fixture.run(true).unwrap();
        assert_eq!(pruned.removed_sessions, 1);
        assert!(pruned.budget_verified && latest.exists() && !working.exists());
        assert!(latest.parent().unwrap().join("s-2-1.lock").is_file());
        assert_eq!(fixture.run(true).unwrap().removed_sessions, 0);
    }
}

#[test]
fn working_sessions_require_age_and_an_existing_uncontended_lease() {
    for shared in [true, false] {
        let fixture = Fixture::new();
        let working = fixture.session("s-1-1-working");
        let lock_path = working.parent().unwrap().join("s-1-1.lock");
        let lease = File::open(&lock_path).unwrap();
        if shared {
            lease.lock_shared().unwrap();
        } else {
            lease.lock().unwrap();
        }
        let busy = fixture.run(true).unwrap();
        assert_eq!(busy.removed_sessions, 0);
        assert_eq!(busy.unmeasured_sessions, 1);
        assert!(!busy.budget_verified && working.exists());
        drop(lease);
        for now in [UNIX_EPOCH, UNIX_EPOCH + Duration::from_secs(299)] {
            let young = maintain(&fixture.0, true, Policy::default(), now).unwrap();
            assert_eq!(young.removed_sessions, 0);
            assert!(working.exists());
        }
        fs::remove_file(lock_path).unwrap();
        let missing = fixture.run(true).unwrap();
        assert_eq!(missing.removed_sessions, 0);
        assert!(!missing.budget_verified && working.exists());
    }
}

#[test]
fn unknown_working_payload_blocks_all_retirement() {
    for kind in ["unknown", "symlink", "nested"] {
        let fixture = Fixture::new();
        let working = fixture.session("s-1-1-working");
        let old = fixture.session("s-2-1-aaa");
        fixture.session("s-3-1-bbb");
        match kind {
            "unknown" => fs::write(working.join("source.rs"), "not cache").unwrap(),
            "symlink" => symlink(&fixture.0, working.join("extra.o")).unwrap(),
            "nested" => fs::create_dir(working.join("extra.o")).unwrap(),
            _ => unreachable!(),
        }
        assert!(fixture.run(true).is_err(), "{kind}");
        assert!(working.exists() && old.exists());
    }
}

#[test]
fn interrupted_working_retirement_recovers_without_a_header() {
    let fixture = Fixture::new();
    let working = fixture.session("s-1-1-working");
    let latest = fixture.session("s-2-1-aaa");
    let retired = fixture.profile().join(layout::RETIRED);
    fs::create_dir(&retired).unwrap();
    let inventory = inspect(
        &fixture.profile(),
        Policy::default(),
        UNIX_EPOCH + Duration::from_secs(3600),
    )
    .unwrap();
    assert_eq!(inventory.candidates.len(), 1);
    begin_retirement(&inventory.candidates[0]).unwrap();
    let destination = retired.join("demo-123--s-1-1-working");
    fs::remove_file(destination.join("dep-graph.bin")).unwrap();
    drop(inventory);
    let audit = fixture.run(false).unwrap();
    assert_eq!(audit.retired_sessions, 1);
    assert!(!audit.budget_verified);
    let resumed = fixture.run(true).unwrap();
    assert_eq!(resumed.recovered_sessions, 1);
    assert!(resumed.budget_verified && latest.exists());
    assert!(!working.exists() && !destination.exists());
}

#[test]
fn killed_real_rustc_writer_is_reclaimed_without_losing_completed_cache() {
    let fixture = Fixture::new();
    let source = "pub fn answer() -> u64 { 42 }\n";
    fs::write(fixture.0.join("probe.rs"), source).unwrap();
    rustc_build(&fixture);
    let artifact = fs::read(fixture.profile().join("probe.rlib")).unwrap();
    let objects = current_objects(&fixture);
    assert!(!objects.is_empty());

    // A proc macro parks *inside rustc*, after incremental-session admission.
    // This avoids a timing race with a fast compiler or a synthetic lease owner.
    fs::write(
        fixture.0.join("hold.rs"),
        r#"extern crate proc_macro;
#[proc_macro]
pub fn item(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    std::fs::write(std::env::var_os("TERLAN_CACHE_PROBE_READY").unwrap(), "ready").unwrap();
    loop { std::thread::park(); }
}
"#,
    )
    .unwrap();
    let mut macro_build = Compiler(
        Command::new("rustc")
            .args(["--crate-name", "hold", "--crate-type", "proc-macro", "-o"])
            .arg(fixture.0.join("libhold.so"))
            .arg(fixture.0.join("hold.rs"))
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_compiler(&mut macro_build);
    fs::write(
        fixture.0.join("probe.rs"),
        "extern crate hold; hold::item!();",
    )
    .unwrap();
    let ready = fixture.0.join("writer.ready");
    let mut writer = Compiler(
        Command::new("rustc")
            .args(["--crate-name", "cache_probe", "--crate-type", "rlib", "-C"])
            .arg(format!(
                "incremental={}",
                fixture.profile().join("incremental").display()
            ))
            .args(["-C", "debuginfo=line-tables-only", "--extern"])
            .arg(format!("hold={}", fixture.0.join("libhold.so").display()))
            .arg("-o")
            .arg(fixture.profile().join("probe.rlib"))
            .arg(fixture.0.join("probe.rs"))
            .env("TERLAN_CACHE_PROBE_READY", &ready)
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready.exists() {
        assert!(
            writer.0.try_wait().unwrap().is_none(),
            "writer exited before ready"
        );
        assert!(
            Instant::now() < deadline,
            "writer did not enter macro in 30s"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let now = SystemTime::now() + Duration::from_secs(301);
    let busy = maintain(&fixture.0, true, Policy::default(), now).unwrap();
    assert_eq!(busy.removed_sessions, 0);
    assert_eq!(busy.unmeasured_sessions, 1);
    assert!(!busy.budget_verified);
    writer.0.kill().unwrap();
    assert!(!writer.0.wait().unwrap().success());
    let abandoned = maintain(&fixture.0, false, Policy::default(), now).unwrap();
    assert_eq!(abandoned.abandoned_sessions, 1);
    let pruned = maintain(&fixture.0, true, Policy::default(), now).unwrap();
    assert_eq!(pruned.removed_sessions, 1);
    assert_eq!(pruned.abandoned_sessions, 1);
    assert!(pruned.budget_verified);
    assert_eq!(
        fs::read(fixture.profile().join("probe.rlib")).unwrap(),
        artifact
    );
    assert_eq!(current_objects(&fixture), objects);
    fs::write(fixture.0.join("probe.rs"), source).unwrap();
    rustc_build(&fixture);
    assert_eq!(current_objects(&fixture), objects);
}
