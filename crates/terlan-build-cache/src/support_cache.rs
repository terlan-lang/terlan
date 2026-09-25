//! Retention for private support-tool Cargo generations, never compiler outputs.
//!
//! The bootstrap and this cleaner share one exclusive lock. A generation is
//! writable only while that lock is held; the installed current generation is
//! pinned. This is a trusted-local-filesystem protocol, not a hostile-user jail.
use crate::layout;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

#[path = "support_cache_inventory.rs"]
mod inventory;

#[cfg(test)]
#[path = "support_cache_test.rs"]
mod tests;

#[derive(Clone, Copy)]
struct Policy {
    bytes: u64,
    entries: usize,
    generations: usize,
    maximum_age: Duration,
    grace: Duration,
    lock_wait: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            bytes: 4 * 1024 * 1024 * 1024,
            entries: 100_000,
            generations: 3,
            maximum_age: Duration::from_secs(7 * 24 * 3600),
            grace: Duration::from_secs(300),
            lock_wait: Duration::from_secs(110),
        }
    }
}

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    current: String,
    allocated_bytes_before: u64,
    allocated_bytes_after: u64,
    entries_before: usize,
    entries_after: usize,
    generations_before: usize,
    generations_after: usize,
    byte_budget: u64,
    entry_budget: usize,
    generation_budget: usize,
    maximum_age_seconds: u64,
    grace_seconds: u64,
    retired: Vec<String>,
    recovered: Vec<String>,
    budget_verified: bool,
}

/// Run only after bootstrap installation, acquiring rather than assuming its lease.
pub(crate) fn run(root: &Path) -> io::Result<bool> {
    let report = maintain(root, Policy::default(), SystemTime::now())?;
    println!("{}", serde_json::to_string(&report)?);
    if !report.budget_verified {
        eprintln!(
            "error[build.cache.support]: protected support cache exceeds its retention budget"
        );
    }
    Ok(report.budget_verified)
}

fn maintain(root: &Path, policy: Policy, now: SystemTime) -> io::Result<Report> {
    layout::regular(&root.join("Cargo.toml"))?;
    if !root.join(".git").try_exists()? {
        return Err(io::Error::other(
            "support retention requires a Git checkout",
        ));
    }
    layout::directory(&root.join("target"))?;
    layout::directory(&root.join("target/quality"))?;
    let start = Instant::now();
    let _lease = loop {
        if let Some(lease) = layout::lease(&root.join("target/quality/bootstrap-owner.lock"))? {
            break lease;
        }
        if start.elapsed() >= policy.lock_wait {
            return Err(io::Error::other("support bootstrap or retention is active"));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let cache = root.join("target/hermetic-support");
    let mut snapshot = inventory::inspect(&cache)?;
    let current = snapshot.current.clone();
    let (bytes_before, entries_before) = totals(&snapshot.generations, &BTreeSet::new())?;
    let generations_before = snapshot.generations.len();
    // Inspection validates the entire namespace before the first mutation,
    // including residue from an interrupted rename/unlink transaction.
    let mut recovered = Vec::new();
    for generation in &snapshot.generations {
        if generation.recovering {
            inventory::remove(&cache, generation)?;
            recovered.push(generation.key.clone());
        }
    }
    snapshot.generations.retain(|entry| !entry.recovering);
    let retired = plan(&snapshot.generations, &current, policy, now)?;
    for generation in &snapshot.generations {
        if retired.contains(&generation.key) {
            let directory = cache.join("retired");
            if !layout::present(&directory)? {
                fs::create_dir(&directory)?;
            }
            layout::directory(&directory)?;
            let destination = directory.join(&generation.key);
            if layout::present(&destination)? {
                return Err(io::Error::other(
                    "support retirement destination is occupied",
                ));
            }
            fs::rename(&generation.path, &destination)?;
            // On interruption the reserved retired/<key> path is recovered
            // under the same lease. Never rename the current live generation.
            inventory::remove_path(&cache, &destination)?;
        }
    }
    let after = inventory::inspect(&cache)?;
    if after.current != current {
        return Err(io::Error::other(
            "support current generation changed under its lease",
        ));
    }
    let (bytes_after, entries_after) = totals(&after.generations, &BTreeSet::new())?;
    Ok(Report {
        schema: "terlan.support-cache-retention.v1",
        current,
        allocated_bytes_before: bytes_before,
        allocated_bytes_after: bytes_after,
        entries_before,
        entries_after,
        generations_before,
        generations_after: after.generations.len(),
        byte_budget: policy.bytes,
        entry_budget: policy.entries,
        generation_budget: policy.generations,
        maximum_age_seconds: policy.maximum_age.as_secs(),
        grace_seconds: policy.grace.as_secs(),
        retired: retired.into_iter().collect(),
        recovered,
        budget_verified: bytes_after <= policy.bytes
            && entries_after <= policy.entries
            && after.generations.len() <= policy.generations,
    })
}

fn totals(
    entries: &[inventory::Generation],
    retired: &BTreeSet<String>,
) -> io::Result<(u64, usize)> {
    let mut inodes = BTreeMap::new();
    let mut paths = 0_usize;
    for entry in entries.iter().filter(|entry| !retired.contains(&entry.key)) {
        paths = paths
            .checked_add(entry.entries)
            .ok_or_else(|| io::Error::other("support cache entry count overflow"))?;
        inodes.extend(entry.inodes.iter().map(|(key, bytes)| (*key, *bytes)));
    }
    let bytes = inodes.values().try_fold(0_u64, |total, bytes| {
        total
            .checked_add(*bytes)
            .ok_or_else(|| io::Error::other("support cache byte count overflow"))
    })?;
    Ok((bytes, paths))
}

fn plan(
    generations: &[inventory::Generation],
    current: &str,
    policy: Policy,
    now: SystemTime,
) -> io::Result<BTreeSet<String>> {
    let mut candidates: Vec<_> = generations
        .iter()
        .filter(|entry| entry.key != current)
        .filter(|entry| {
            now.duration_since(entry.modified)
                .is_ok_and(|age| age >= policy.grace)
        })
        .collect();
    candidates.sort_by(|a, b| (a.modified, &a.key).cmp(&(b.modified, &b.key)));
    let mut retired = BTreeSet::new();
    for entry in candidates {
        let (bytes, count) = totals(generations, &retired)?;
        let expired = now
            .duration_since(entry.modified)
            .is_ok_and(|age| age >= policy.maximum_age);
        if entry.key.starts_with("legacy-")
            || expired
            || bytes > policy.bytes
            || count > policy.entries
            || generations.len() - retired.len() > policy.generations
        {
            retired.insert(entry.key.clone());
        }
    }
    Ok(retired)
}

fn key(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn legacy(value: &str) -> bool {
    matches!(value, "legacy-target" | "legacy-registry" | "legacy-git")
}

fn invalid(path: &Path) -> io::Error {
    io::Error::other(format!("unsafe support-cache entry: {}", path.display()))
}
