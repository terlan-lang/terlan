//! Retire obsolete and abandoned sessions under rustc leases, with recoverable names.
use crate::layout::{self, SessionName};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[cfg(test)]
#[path = "incremental_test.rs"]
mod incremental_test;

/// Preserve current/recent configurations and all sessions leased by rustc.
#[derive(Clone, Copy)]
pub(crate) struct Policy {
    pub(crate) bytes: u64,
    pub(crate) sessions: usize,
    pub(crate) grace: Duration,
    pub(crate) configuration_age: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            bytes: 64 * 1024 * 1024 * 1024,
            sessions: 1024,
            grace: Duration::from_secs(300),
            configuration_age: Duration::from_secs(72 * 3600),
        }
    }
}

/// Byte counts deduplicate inodes within this namespace, not filesystem free space.
#[derive(Default, Serialize)]
pub(crate) struct Report {
    schema: &'static str,
    mode: &'static str,
    pub(crate) sessions_before: usize,
    pub(crate) sessions_after: usize,
    pub(crate) allocated_bytes_before: u64,
    pub(crate) allocated_bytes_after: u64,
    pub(crate) redundant_sessions: usize,
    pub(crate) abandoned_sessions: usize,
    pub(crate) superseded_configurations: usize,
    pub(crate) removed_sessions: usize,
    pub(crate) recovered_sessions: usize,
    pub(crate) protected_sessions: usize,
    pub(crate) unmeasured_sessions: usize,
    pub(crate) retired_sessions: usize,
    byte_budget: u64,
    session_budget: usize,
    grace_seconds: u64,
    configuration_age_seconds: u64,
    pub(crate) budget_verified: bool,
    candidate_paths: Vec<PathBuf>,
}

struct Candidate {
    path: PathBuf,
    destination: PathBuf,
    // An interrupted writer may never have finished a versioned dep-graph header.
    working: bool,
    superseded: bool,
    // Held from inspection through rename; rustc readers use a shared lease.
    _lease: layout::Lease,
}

#[derive(Default)]
struct Inventory {
    sessions: usize,
    protected: usize,
    unmeasured: usize,
    files: HashMap<(u64, u64), u64>,
    candidates: Vec<Candidate>,
    retired: Vec<PathBuf>,
}

impl Inventory {
    fn bytes(&self) -> u64 {
        self.files.values().sum()
    }

    fn record(&mut self, path: &Path, header: bool) -> io::Result<()> {
        for (_, metadata) in layout::payload(path, header)? {
            self.files
                .insert((metadata.dev(), metadata.ino()), metadata.blocks() * 512);
        }
        Ok(())
    }
}

/// Audit without filesystem writes, or explicitly prune only leased obsolete sessions.
pub(crate) fn maintain(
    repo: &Path,
    prune: bool,
    policy: Policy,
    now: SystemTime,
) -> io::Result<Report> {
    let profile = layout::profile_root(repo)?;
    let owner_path = profile.join(layout::OWNER_LOCK);
    let _owner = if prune {
        match File::create_new(&owner_path) {
            Ok(file) => drop(file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        Some(
            layout::lease(&owner_path)?
                .ok_or_else(|| io::Error::other("cache maintenance already active"))?,
        )
    } else if layout::present(&owner_path)? {
        // Audit neither creates a lock nor races this tool's retirement transactions.
        Some(
            layout::lease(&owner_path)?
                .ok_or_else(|| io::Error::other("cache maintenance already active"))?,
        )
    } else {
        None
    };
    let inventory = inspect(&profile, policy, now)?;
    let mut report = Report {
        schema: "terlan.rust-incremental-retention.v1",
        mode: if prune { "prune" } else { "audit" },
        sessions_before: inventory.sessions,
        allocated_bytes_before: inventory.bytes(),
        redundant_sessions: inventory
            .candidates
            .iter()
            .filter(|entry| !entry.working && !entry.superseded)
            .count(),
        abandoned_sessions: inventory
            .candidates
            .iter()
            .filter(|entry| entry.working)
            .count(),
        superseded_configurations: inventory
            .candidates
            .iter()
            .filter(|entry| entry.superseded)
            .count(),
        retired_sessions: inventory.retired.len(),
        byte_budget: policy.bytes,
        session_budget: policy.sessions,
        grace_seconds: policy.grace.as_secs(),
        configuration_age_seconds: policy.configuration_age.as_secs(),
        candidate_paths: inventory
            .candidates
            .iter()
            .map(|candidate| {
                candidate
                    .path
                    .strip_prefix(repo)
                    .unwrap_or(&candidate.path)
                    .to_path_buf()
            })
            .collect(),
        ..Report::default()
    };
    if !prune {
        finish_report(&mut report, &inventory, policy);
        return Ok(report);
    }
    if prune {
        // inspect validates *all* retired/current namespaces before deleting anything.
        for retired in &inventory.retired {
            layout::remove_payload(retired)?;
            report.recovered_sessions += 1;
        }
        let retired_root = profile.join(layout::RETIRED);
        if !inventory.candidates.is_empty() && !layout::present(&retired_root)? {
            fs::create_dir(&retired_root)?;
        }
        for candidate in &inventory.candidates {
            retire(candidate)?;
            report.removed_sessions += 1;
        }
    }
    // Release leases before re-inspection. Never count our own locks as rustc work.
    drop(inventory);
    let after = inspect(&profile, policy, now)?;
    finish_report(&mut report, &after, policy);
    Ok(report)
}

fn finish_report(report: &mut Report, after: &Inventory, policy: Policy) {
    report.sessions_after = after.sessions;
    report.allocated_bytes_after = after.bytes();
    report.protected_sessions = after.protected;
    report.unmeasured_sessions = after.unmeasured;
    report.budget_verified = after.unmeasured == 0
        && after.retired.is_empty()
        && after.sessions <= policy.sessions
        && after.bytes() <= policy.bytes;
}

fn inspect(profile: &Path, policy: Policy, now: SystemTime) -> io::Result<Inventory> {
    let mut inventory = Inventory::default();
    let retired_root = profile.join(layout::RETIRED);
    if layout::present(&retired_root)? {
        for retired in layout::children(&retired_root)? {
            let name = layout::name(&retired)?;
            let (krate, session) = name.split_once("--").ok_or_else(|| layout::invalid(name))?;
            if !layout::crate_name(krate) {
                return Err(layout::invalid(name));
            }
            layout::session_name(session)?;
            inventory.record(&retired, false)?;
            inventory.retired.push(retired);
        }
    }
    let crates = layout::children(&profile.join("incremental"))?;
    let generations = newest_configurations(&crates)?;
    for krate in crates {
        if !layout::crate_name(layout::name(&krate)?) {
            return Err(layout::invalid(krate.display()));
        }
        let target = layout::name(&krate)?.rsplit_once('-').unwrap().0;
        inspect_crate(
            &krate,
            &retired_root,
            generations.get(target).copied(),
            policy,
            now,
            &mut inventory,
        )?;
    }
    Ok(inventory)
}

fn newest_configurations(crates: &[PathBuf]) -> io::Result<HashMap<String, SystemTime>> {
    let mut generations = HashMap::new();
    for krate in crates {
        let name = layout::name(krate)?;
        if !layout::crate_name(name) {
            return Err(layout::invalid(name));
        }
        let target = name.rsplit_once('-').unwrap().0;
        for path in layout::children(krate)? {
            let name = layout::name(&path)?;
            if layout::lock_name(name) {
                continue;
            }
            let session = layout::session_name(name)?;
            if !session.working {
                generations
                    .entry(target.to_owned())
                    .and_modify(|latest: &mut SystemTime| *latest = (*latest).max(session.created))
                    .or_insert(session.created);
            }
        }
    }
    Ok(generations)
}

fn inspect_crate(
    krate: &Path,
    retired: &Path,
    newest_configuration: Option<SystemTime>,
    policy: Policy,
    now: SystemTime,
    inventory: &mut Inventory,
) -> io::Result<()> {
    let mut sessions: Vec<(PathBuf, SessionName)> = Vec::new();
    for entry in layout::children(krate)? {
        let name = layout::name(&entry)?;
        if layout::lock_name(name) {
            if layout::regular(&entry)?.len() != 0 {
                return Err(layout::invalid(entry.display()));
            }
        } else {
            let parsed = layout::session_name(name)?;
            layout::directory(&entry)?;
            sessions.push((entry, parsed));
        }
    }
    let newest = sessions
        .iter()
        .filter(|(_, name)| !name.working)
        .map(|(_, name)| name.created)
        .max();
    for (path, name) in sessions {
        inventory.sessions += 1;
        let lease = match layout::lease(&krate.join(&name.lock)) {
            Ok(lease) => lease,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // rustc creation or its own GC: no lock means no deletion authority.
                inventory.unmeasured += 1;
                inventory.protected += 1;
                continue;
            }
            Err(error) => return Err(error),
        };
        let Some(lease) = lease else {
            inventory.unmeasured += 1;
            inventory.protected += 1;
            continue;
        };
        // rustc keeps an exclusive session lease throughout writes. A dead
        // writer releases it, but the creation/lock gap still needs the grace.
        // Partial working payloads are disposable, never reusable completed caches.
        inventory.record(&path, !name.working)?;
        let old = now
            .duration_since(name.created)
            .is_ok_and(|age| age >= policy.grace);
        // A distinct, completed configuration must supersede the old one. Age
        // alone never removes a target's only/newest configuration. rustc starts
        // a new timestamped session even when it reuses existing work products.
        let superseded = !name.working
            && newest == Some(name.created)
            && newest_configuration.is_some_and(|latest| latest > name.created && latest <= now)
            && now
                .duration_since(name.created)
                .is_ok_and(|age| age >= policy.configuration_age);
        if (!name.working && newest == Some(name.created) && !superseded) || !old {
            inventory.protected += 1;
            continue;
        }
        let destination = retired.join(format!(
            "{}--{}",
            layout::name(krate)?,
            layout::name(&path)?
        ));
        if layout::present(&destination)? {
            return Err(layout::invalid(format!(
                "retirement collision: {}",
                destination.display()
            )));
        }
        inventory.candidates.push(Candidate {
            path,
            destination,
            working: name.working,
            superseded,
            _lease: lease,
        });
    }
    Ok(())
}

fn retire(candidate: &Candidate) -> io::Result<()> {
    begin_retirement(candidate)?;
    layout::remove_payload(&candidate.destination)
}

fn begin_retirement(candidate: &Candidate) -> io::Result<()> {
    // Flat, fully inspected quiescent payload, protected by rustc's own lock.
    // Once renamed, rustc cannot discover it. The maintenance lease serializes
    // recovery if this process dies during the subsequent individual unlinks.
    layout::payload(&candidate.path, !candidate.working)?;
    fs::rename(&candidate.path, &candidate.destination)
}
