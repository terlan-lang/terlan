//! Execution inventory distinguishes work, successful spawn, and incomplete work.

use super::*;
use crate::support::test_fs::TestDirectory;

fn records(path: &std::path::Path) -> Result<Vec<serde_json::Value>, String> {
    std::fs::read_to_string(path)
        .map_err(|error| error.to_string())?
        .lines()
        .map(|line| serde_json::from_str(line).map_err(|error| error.to_string()))
        .collect()
}

fn start(path: &std::path::Path, operation: Operation) -> Result<Activity, String> {
    Activity::begin_at(Some(path.to_owned()), operation, &"a".repeat(64))
        .map_err(|error| format!("{error:?}"))
}

/// Successful process creation is a separate event from entering the link stage.
#[test]
fn inventory_records_attempt_launch_and_completion_separately() -> Result<(), String> {
    let root = TestDirectory::new("native_activity", "completion");
    let path = root.join("cycle.jsonl");
    let mut activity = start(&path, Operation::NativeLink)?;
    activity
        .spawned(123)
        .map_err(|error| format!("{error:?}"))?;
    activity
        .finish(true, Some(4096))
        .map_err(|error| format!("{error:?}"))?;
    let rows = records(&path)?;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["state"], "started");
    assert!(rows[0]["child_pid"].is_null());
    assert_eq!(rows[1]["state"], "spawned");
    assert_eq!(rows[1]["child_pid"], 123);
    assert_eq!(rows[2]["state"], "completed");
    assert_eq!(rows[2]["artifact_bytes"], 4096);
    assert!(rows.iter().all(|row| row["attempt"] == rows[0]["attempt"]));
    assert!(rows.iter().all(|row| row["operation"] == "native-link"));
    root.close();
    Ok(())
}

/// Failure and early scope exit cannot appear as completed work.
#[test]
fn inventory_preserves_failed_and_abandoned_attempts() -> Result<(), String> {
    let root = TestDirectory::new("native_activity", "failure");
    let path = root.join("cycle.jsonl");
    start(&path, Operation::ApplicationObject)?
        .finish(false, None)
        .map_err(|error| format!("{error:?}"))?;
    drop(start(&path, Operation::ModuleObject)?);
    let rows = records(&path)?;
    assert_eq!(
        rows.iter()
            .map(|row| row["state"].as_str())
            .collect::<Vec<_>>(),
        [
            Some("started"),
            Some("failed"),
            Some("started"),
            Some("abandoned")
        ]
    );
    assert_ne!(rows[0]["attempt"], rows[2]["attempt"]);
    assert!(rows.iter().all(|row| row["child_pid"].is_null()));
    root.close();
    Ok(())
}

/// Inventory writes fail closed without inventing output parents or growing forever.
#[test]
fn inventory_rejects_unwritable_invalid_and_exhausted_logs() -> Result<(), String> {
    let root = TestDirectory::new("native_activity", "limits");
    assert!(start(&root.join("missing/cycle.jsonl"), Operation::NativeLink).is_err());
    assert!(Activity::begin_at(
        Some(root.join("invalid.jsonl")),
        Operation::NativeLink,
        "not-a-digest"
    )
    .is_err());
    let path = root.join("full.jsonl");
    std::fs::File::create(&path)
        .and_then(|file| file.set_len(MAX_LOG_BYTES))
        .map_err(|error| error.to_string())?;
    assert!(start(&path, Operation::NativeLink).is_err());
    assert_eq!(
        std::fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len(),
        MAX_LOG_BYTES
    );
    root.close();
    Ok(())
}

/// Concurrent native-unit workers retain distinct attempts and intact JSON rows.
#[test]
fn inventory_serializes_concurrent_workers_without_losing_events() -> Result<(), String> {
    let root = TestDirectory::new("native_activity", "concurrent");
    let path = root.join("cycle.jsonl");
    std::thread::scope(|scope| -> Result<(), String> {
        let threads = (0..8)
            .map(|_| {
                let path = &path;
                scope.spawn(move || {
                    start(path, Operation::ModuleObject)?
                        .finish(true, Some(128))
                        .map_err(|error| format!("{error:?}"))
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread
                .join()
                .map_err(|_| "inventory writer panicked".to_owned())??;
        }
        Ok(())
    })?;
    let rows = records(&path)?;
    assert_eq!(rows.len(), 16);
    let attempts = rows
        .iter()
        .map(|row| row["attempt"].as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(attempts.len(), 8);
    assert_eq!(
        rows.iter()
            .filter(|row| row["state"] == "completed")
            .count(),
        8
    );
    root.close();
    Ok(())
}
