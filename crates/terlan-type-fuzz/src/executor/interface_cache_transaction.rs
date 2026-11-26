use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::artifacts::atomic_write;
use super::oracle::{self, OracleResult};
use crate::{GeneratedCase, Oracle};

const FAMILY: &str = "interface_cache_transaction";
const EVIDENCE: &str = "interface-cache-transaction.json";

/// Proves that a failed interface build cannot poison a previously valid cache.
pub(super) fn run(
    directory: &Path,
    case: &GeneratedCase,
    compiler: &Path,
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
) -> Result<(), String> {
    if case.manifest.family != FAMILY || case.manifest.oracle == Oracle::None {
        return Ok(());
    }
    let workspace = tempfile::Builder::new()
        .prefix(".interface-cache-transaction-")
        .tempdir_in(directory)
        .map_err(|error| format!("create interface cache transaction: {error}"))?;
    stage(workspace.path(), case)?;
    let source = workspace.path().join("CaseTest.terl");
    let initial = check(compiler, &source, timeout_ms, rss_limit_bytes, cpu_limit_ms);
    require(&initial, "passed", "initial interface build")?;
    let before = interface_state(workspace.path())?;
    if before.is_empty() {
        return Err("initial interface build produced no cache state".into());
    }

    mutate(workspace.path(), case)?;
    let failed = check(compiler, &source, timeout_ms, rss_limit_bytes, cpu_limit_ms);
    require(&failed, "failed", "invalid interface build")?;
    let after = interface_state(workspace.path())?;
    if after != before {
        return Err("failed interface build changed the previous cache generation".into());
    }

    stage_sources(workspace.path(), case)?;
    let recovered = check(compiler, &source, timeout_ms, rss_limit_bytes, cpu_limit_ms);
    require(&recovered, "passed", "post-failure interface recovery")?;
    let evidence = serde_json::json!({
        "schema": "terlan.type-fuzz-interface-cache-transaction.v1",
        "case_id": case.manifest.case_id,
        "cache_entries": before.len(),
        "initial_outcome": initial.outcome,
        "failed_outcome": failed.outcome,
        "recovered_outcome": recovered.outcome,
        "failed_diagnostics": failed.stderr,
    });
    let mut bytes = serde_json::to_vec(&evidence).expect("cache evidence serializes");
    bytes.push(b'\n');
    atomic_write(&directory.join(EVIDENCE), &bytes)
}

fn check(
    compiler: &Path,
    source: &Path,
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
) -> OracleResult {
    oracle::run(
        compiler,
        Oracle::Check,
        source,
        timeout_ms,
        rss_limit_bytes,
        cpu_limit_ms,
    )
}

fn require(result: &OracleResult, expected: &str, phase: &str) -> Result<(), String> {
    if result.outcome == expected {
        Ok(())
    } else {
        Err(format!(
            "{phase} ended as {}, expected {expected}: {}",
            result.outcome, result.stderr
        ))
    }
}

fn stage(workspace: &Path, case: &GeneratedCase) -> Result<(), String> {
    fs::write(workspace.join("terlan.toml"), &case.project)
        .map_err(|error| format!("stage cache transaction manifest: {error}"))?;
    fs::write(workspace.join("CaseTest.terl"), &case.source)
        .map_err(|error| format!("stage cache transaction entry: {error}"))?;
    stage_sources(workspace, case)
}

fn stage_sources(workspace: &Path, case: &GeneratedCase) -> Result<(), String> {
    for source in &case.support_sources {
        let path = workspace.join(&source.path);
        let parent = path
            .parent()
            .expect("generated support source has a parent");
        fs::create_dir_all(parent)
            .map_err(|error| format!("stage cache transaction source root: {error}"))?;
        fs::write(&path, &source.source)
            .map_err(|error| format!("stage cache transaction source: {error}"))?;
    }
    Ok(())
}

fn mutate(workspace: &Path, case: &GeneratedCase) -> Result<(), String> {
    let mut sources = case.support_sources.iter().collect::<Vec<_>>();
    sources.sort_by(|left, right| left.path.cmp(&right.path));
    let first = sources
        .first()
        .ok_or_else(|| "cache transaction requires support sources".to_string())?;
    let last = sources.last().expect("nonempty cache transaction sources");
    fs::write(
        workspace.join(&first.path),
        format!(
            "{}\n\npub cache_generation_probe(): Int -> 1.\n",
            first.source
        ),
    )
    .map_err(|error| format!("mutate early cache transaction source: {error}"))?;
    fs::write(
        workspace.join(&last.path),
        format!("{}\n\npub broken(: Int -> 0.\n", last.source),
    )
    .map_err(|error| format!("mutate failing cache transaction source: {error}"))
}

fn interface_state(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    let cache = root.join(".terlan");
    let mut entries =
        fs::read_dir(&cache).map_err(|error| format!("read interface cache state: {error}"))?;
    interface_state_from_entries(root, &mut entries)
}

fn interface_state_from_entries(
    root: &Path,
    entries: &mut dyn Iterator<Item = std::io::Result<fs::DirEntry>>,
) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    let mut state = BTreeMap::new();
    for entry in entries {
        let path = interface_entry_path(entry)?;
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().contains(".typi"))
        {
            let relative = path
                .strip_prefix(root)
                .expect("cache entry belongs to workspace")
                .to_path_buf();
            let bytes = fs::read(&path)
                .map_err(|error| format!("read interface cache artifact: {error}"))?;
            state.insert(relative, bytes);
        }
    }
    Ok(state)
}

fn interface_entry_path(entry: std::io::Result<fs::DirEntry>) -> Result<PathBuf, String> {
    entry
        .map(|entry| entry.path())
        .map_err(|error| format!("read interface cache entry: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GenerationConfig, RunOptions};

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn cache_case(oracle: Oracle) -> GeneratedCase {
        let options = RunOptions::default();
        crate::generate(
            21,
            &GenerationConfig {
                seed: 1,
                max_depth: 8,
                oracle,
                timeout_ms: options.case_timeout_ms,
                rss_limit_bytes: options.rss_limit_bytes,
                cpu_limit_ms: options.cpu_limit_ms,
                compiler_revision: "test",
            },
        )
    }

    #[test]
    fn unrelated_and_offline_cases_skip_the_transaction() {
        let root = tempfile::tempdir().expect("transaction root");
        let mut unrelated = cache_case(Oracle::Check);
        unrelated.manifest.family = "other".into();
        run(root.path(), &unrelated, Path::new("missing"), 1, 1, 1).expect("unrelated case");
        run(
            root.path(),
            &cache_case(Oracle::None),
            Path::new("missing"),
            1,
            1,
            1,
        )
        .expect("offline case");
        assert!(!root.path().join(EVIDENCE).exists());
    }

    #[cfg(unix)]
    fn compiler(root: &Path, mode: &str) -> PathBuf {
        let script = root.join(format!("compiler-{mode}"));
        let counter = root.join(format!("counter-{mode}"));
        let second = match mode {
            "unexpected-pass" => "exit 0",
            "corrupt" => "printf corrupt > .terlan/demo.typi; exit 1",
            "after-cache-error" => "ln -s missing .terlan/bad.typi; exit 1",
            "recovery-stage-error" => concat!(
                "last=$(find src -type f | sort | tail -1); ",
                "rm -f \"$last\"; mkdir \"$last\"; exit 1"
            ),
            _ => "exit 1",
        };
        let third = if mode == "recovery-fail" {
            "exit 1"
        } else {
            "exit 0"
        };
        let first = if mode == "initial-fail" {
            "exit 1"
        } else if mode == "no-cache" {
            "mkdir -p .terlan; exit 0"
        } else if mode == "before-cache-error" {
            "mkdir -p .terlan; ln -s missing .terlan/bad.typi; exit 0"
        } else {
            concat!(
                "mkdir -p .terlan; ",
                "printf stable > .terlan/demo.typi; ",
                "printf stable > .terlan/demo.typi.hash; exit 0"
            )
        };
        fs::write(
            &script,
            format!(
                "#!/bin/sh\ncount=0\ntest ! -f '{counter}' || count=$(cat '{counter}')\ncount=$((count + 1))\nprintf '%s' \"$count\" > '{counter}'\ncase $count in 1) {first} ;; 2) {second} ;; *) {third} ;; esac\n",
                counter = counter.display(),
            ),
        )
        .expect("compiler script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700))
            .expect("compiler permissions");
        script
    }

    #[cfg(unix)]
    fn permissive_compiler(root: &Path) -> PathBuf {
        let script = root.join("permissive-compiler");
        fs::write(
            &script,
            concat!(
                "#!/bin/sh\n",
                "find . -path '*/src/*' -name '*.terl' -type f | while read -r source; do\n",
                "  module=$(sed -n 's/^module \\([^ ]*\\)\\.$/\\1/p' \"$source\")\n",
                "  test -n \"$module\" || continue\n",
                "  project=${source%%/src/*}\n",
                "  mkdir -p \"$project/.terlan\"\n",
                "  cp \"$source\" \"$project/.terlan/$module.typi\"\n",
                "done\n",
            ),
        )
        .expect("permissive compiler");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700))
            .expect("permissive compiler permissions");
        script
    }

    #[cfg(unix)]
    #[test]
    fn cache_transaction_preserves_and_recovers_the_previous_generation() {
        let root = tempfile::tempdir().expect("transaction root");
        run(
            root.path(),
            &cache_case(Oracle::Check),
            &compiler(root.path(), "success"),
            1_000,
            1 << 30,
            1_000,
        )
        .expect("cache transaction");
        let evidence = fs::read_to_string(root.path().join(EVIDENCE)).expect("cache evidence");
        assert!(evidence.contains("\"initial_outcome\":\"passed\""));
        assert!(evidence.contains("\"failed_outcome\":\"failed\""));
        assert!(evidence.contains("\"recovered_outcome\":\"passed\""));
    }

    #[cfg(unix)]
    #[test]
    fn every_transaction_failure_is_attributed() {
        for (mode, expected) in [
            ("initial-fail", "initial interface build ended as failed"),
            ("no-cache", "produced no cache state"),
            ("unexpected-pass", "invalid interface build ended as passed"),
            ("corrupt", "changed the previous cache generation"),
            ("before-cache-error", "read interface cache artifact"),
            ("after-cache-error", "read interface cache artifact"),
            ("recovery-stage-error", "stage cache transaction source"),
            (
                "recovery-fail",
                "post-failure interface recovery ended as failed",
            ),
        ] {
            let root = tempfile::tempdir().expect("transaction root");
            let error = run(
                root.path(),
                &cache_case(Oracle::Check),
                &compiler(root.path(), mode),
                1_000,
                1 << 30,
                1_000,
            )
            .unwrap_err();
            assert!(error.contains(expected), "{mode}: {error}");
        }
        let missing = tempfile::tempdir().expect("missing root");
        assert!(run(
            &missing.path().join("absent"),
            &cache_case(Oracle::Check),
            Path::new("missing"),
            1,
            1,
            1,
        )
        .unwrap_err()
        .contains("create interface cache transaction"));

        let root = tempfile::tempdir().expect("empty support root");
        let mut empty = cache_case(Oracle::Check);
        empty.support_sources.clear();
        assert!(run(
            root.path(),
            &empty,
            &compiler(root.path(), "empty-support"),
            1_000,
            1 << 30,
            1_000,
        )
        .unwrap_err()
        .contains("requires support sources"));

        let root = tempfile::tempdir().expect("invalid support root");
        let mut invalid = cache_case(Oracle::Check);
        invalid.support_sources[0].path = PathBuf::from(format!("src/{}/A.terl", "x".repeat(300)));
        assert!(run(root.path(), &invalid, Path::new("missing"), 1, 1, 1,)
            .unwrap_err()
            .contains("stage cache transaction source root"));

        let root = tempfile::tempdir().expect("evidence failure root");
        fs::create_dir(root.path().join(EVIDENCE)).expect("block evidence");
        assert!(run(
            root.path(),
            &cache_case(Oracle::Check),
            &compiler(root.path(), "evidence-fail"),
            1_000,
            1 << 30,
            1_000,
        )
        .unwrap_err()
        .contains("publish"));
    }

    #[test]
    fn staging_mutation_and_cache_io_failures_are_attributed() {
        let root = tempfile::tempdir().expect("transaction root");
        let case = cache_case(Oracle::Check);
        stage(root.path(), &case).expect("stage valid case");
        mutate(root.path(), &case).expect("mutate valid case");
        fs::create_dir(root.path().join(".terlan")).expect("empty cache");
        fs::write(root.path().join(".terlan/ignored.txt"), "ignored").expect("ignored cache entry");
        assert!(interface_state(root.path())
            .expect("empty cache")
            .is_empty());

        let blocked = tempfile::tempdir().expect("blocked root");
        fs::create_dir(blocked.path().join("terlan.toml")).expect("block manifest");
        assert!(stage(blocked.path(), &case)
            .unwrap_err()
            .contains("stage cache transaction manifest"));

        let blocked = tempfile::tempdir().expect("blocked entry root");
        fs::create_dir(blocked.path().join("CaseTest.terl")).expect("block entry");
        assert!(stage(blocked.path(), &case)
            .unwrap_err()
            .contains("stage cache transaction entry"));

        let mut empty = case.clone();
        empty.support_sources.clear();
        assert!(mutate(root.path(), &empty)
            .unwrap_err()
            .contains("requires support sources"));
        assert!(interface_state(&root.path().join("missing"))
            .unwrap_err()
            .contains("read interface cache state"));
        assert!(interface_entry_path(Err(std::io::Error::other("entry")))
            .unwrap_err()
            .contains("read interface cache entry"));
        let mut entries = [Err(std::io::Error::other("entry"))].into_iter();
        assert!(interface_state_from_entries(root.path(), &mut entries)
            .unwrap_err()
            .contains("read interface cache entry"));

        let blocked = tempfile::tempdir().expect("blocked source root");
        fs::write(blocked.path().join("src"), "block").expect("block source root");
        assert!(stage_sources(blocked.path(), &case)
            .unwrap_err()
            .contains("stage cache transaction source root"));

        let blocked = tempfile::tempdir().expect("blocked source");
        let source = &case.support_sources[0];
        fs::create_dir_all(
            blocked
                .path()
                .join(source.path.parent().expect("source parent")),
        )
        .expect("source parent");
        fs::create_dir(blocked.path().join(&source.path)).expect("block source");
        assert!(stage_sources(blocked.path(), &case)
            .unwrap_err()
            .contains("stage cache transaction source"));

        let blocked = tempfile::tempdir().expect("blocked mutation");
        stage(blocked.path(), &case).expect("stage mutation case");
        fs::remove_file(blocked.path().join(&case.support_sources[0].path))
            .expect("remove first source");
        fs::create_dir(blocked.path().join(&case.support_sources[0].path))
            .expect("block first mutation");
        assert!(mutate(blocked.path(), &case)
            .unwrap_err()
            .contains("mutate early cache transaction source"));

        let blocked = tempfile::tempdir().expect("blocked final mutation");
        stage(blocked.path(), &case).expect("stage final mutation case");
        let last = case.support_sources.last().expect("last source");
        fs::remove_file(blocked.path().join(&last.path)).expect("remove last source");
        fs::create_dir(blocked.path().join(&last.path)).expect("block final mutation");
        assert!(mutate(blocked.path(), &case)
            .unwrap_err()
            .contains("mutate failing cache transaction source"));
    }

    #[cfg(unix)]
    #[test]
    fn transaction_failures_propagate_through_replay_and_batch_runs() {
        let replay_root = tempfile::tempdir().expect("replay root");
        let case = cache_case(Oracle::Check);
        super::super::artifacts::write_case(replay_root.path(), &case).expect("replay case");
        assert!(super::super::artifacts::replay(
            replay_root.path(),
            &permissive_compiler(replay_root.path()),
        )
        .unwrap_err()
        .contains("invalid interface build ended as passed"));

        let batch_root = tempfile::tempdir().expect("batch root");
        let options = RunOptions {
            cases: 22,
            oracle: Oracle::Check,
            out_dir: batch_root.path().join("cases"),
            case_timeout_ms: 1_000,
            total_timeout_ms: 10_000,
            cpu_limit_ms: 1_000,
            ..RunOptions::default()
        };
        assert!(super::super::artifacts::generate_run(
            &options,
            &permissive_compiler(batch_root.path()),
            "test",
        )
        .unwrap_err()
        .contains("invalid interface build ended as passed"));
    }
}
