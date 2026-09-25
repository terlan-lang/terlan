//! Bounded, no-follow inventory and recoverable support-cache retirement.
use super::{invalid, key, layout, legacy};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_ENTRIES: usize = 300_000;
const MAX_GENERATIONS: usize = 128;
const MAX_DEPTH: usize = 128;

pub(super) struct Generation {
    pub(super) path: PathBuf,
    pub(super) key: String,
    pub(super) entries: usize,
    pub(super) inodes: BTreeMap<(u64, u64), u64>,
    pub(super) modified: SystemTime,
    pub(super) recovering: bool,
}

pub(super) struct Snapshot {
    pub(super) current: String,
    pub(super) generations: Vec<Generation>,
}

pub(super) fn inspect(cache: &Path) -> io::Result<Snapshot> {
    layout::directory(cache)?;
    let device = fs::symlink_metadata(cache)?.dev();
    let active = cache.join("active");
    if layout::regular(&active)?.len() != 65 {
        return Err(invalid(&active));
    }
    let text = fs::read_to_string(&active)?;
    let current = text.strip_suffix('\n').ok_or_else(|| invalid(&active))?;
    if !key(current) {
        return Err(invalid(&active));
    }
    let mut generations = Vec::new();
    let mut total = 0;
    for child in layout::children(cache)? {
        match layout::name(&child)? {
            "active" => {}
            "generations" | "retired" => {
                let recovering = child.ends_with("retired");
                for path in layout::children(&child)? {
                    if generations.len() >= MAX_GENERATIONS {
                        return Err(io::Error::other(
                            "support cache generation scan bound exceeded",
                        ));
                    }
                    let name = layout::name(&path)?;
                    if !(key(name) || recovering && legacy(name)) {
                        return Err(invalid(&path));
                    }
                    if !legacy(name) {
                        for entry in layout::children(&path)? {
                            match layout::name(&entry)? {
                                "target" | "registry" | "git" => layout::directory(&entry)?,
                                "last-used" => {
                                    layout::regular(&entry)?;
                                }
                                _ => return Err(invalid(&entry)),
                            }
                        }
                    }
                    let generation =
                        measure(&path, name.to_owned(), recovering, device, &mut total)?;
                    generations.push(generation);
                }
            }
            name @ ("target" | "registry" | "git") => {
                // Previous bootstrap layout: never mounted by the new producer.
                // It becomes disposable only after a new current key is installed.
                generations.push(measure(
                    &child,
                    format!("legacy-{name}"),
                    false,
                    device,
                    &mut total,
                )?);
            }
            _ => return Err(invalid(&child)),
        }
        if generations.len() > MAX_GENERATIONS {
            return Err(io::Error::other(
                "support cache generation scan bound exceeded",
            ));
        }
    }
    let current_path = cache.join("generations").join(current);
    layout::regular(&current_path.join("last-used"))?;
    for relative in ["target", "registry", "git"] {
        layout::directory(&current_path.join(relative))?;
    }
    // A marker alone cannot pin a missing or unsuccessful producer. The owner
    // has already checked byte identity before installation; retention only
    // checks that these outputs/receipt still exist, never manufactures evidence.
    for relative in [
        "target/debug/terlan-build-cache",
        "target/debug/terlan-test-orchestrator",
        "target/quality/hermetic-support.json",
    ] {
        let path = current_path.join(relative);
        for ancestor in path.ancestors().skip(1).take_while(|path| *path != cache) {
            layout::directory(ancestor)?;
        }
        layout::regular(&path)?;
    }
    Ok(Snapshot {
        current: current.to_owned(),
        generations,
    })
}

fn measure(
    path: &Path,
    name: String,
    recovering: bool,
    device: u64,
    total: &mut usize,
) -> io::Result<Generation> {
    layout::directory(path)?;
    let mut result = Generation {
        path: path.into(),
        key: name,
        entries: 0,
        inodes: BTreeMap::new(),
        modified: SystemTime::UNIX_EPOCH,
        recovering,
    };
    let mut pending = vec![(path.to_owned(), 0)];
    while let Some((entry, depth)) = pending.pop() {
        *total += 1;
        if *total > MAX_ENTRIES || depth > MAX_DEPTH {
            return Err(io::Error::other("support cache tree scan bound exceeded"));
        }
        let metadata = fs::symlink_metadata(&entry)?;
        if metadata.dev() != device
            || !(metadata.is_dir() || metadata.is_file() || metadata.is_symlink())
        {
            return Err(invalid(&entry));
        }
        result.entries += 1;
        result.modified = result.modified.max(metadata.modified()?);
        let bytes = metadata
            .blocks()
            .checked_mul(512)
            .ok_or_else(|| invalid(&entry))?;
        result
            .inodes
            .insert((metadata.dev(), metadata.ino()), bytes);
        if metadata.is_dir() {
            for child in fs::read_dir(&entry)? {
                pending.push((child?.path(), depth + 1));
                if pending.len() + *total > MAX_ENTRIES {
                    return Err(io::Error::other("support cache tree scan bound exceeded"));
                }
            }
        }
    }
    Ok(result)
}

pub(super) fn remove(cache: &Path, generation: &Generation) -> io::Result<()> {
    remove_path(cache, &generation.path)
}

pub(super) fn remove_path(cache: &Path, path: &Path) -> io::Result<()> {
    if path.parent() != Some(cache.join("retired").as_path()) {
        return Err(invalid(path));
    }
    layout::directory(path)?;
    // std's no-follow recursive removal unlinks interior symlinks, not their
    // targets. All contents were inspected under the same bootstrap lease.
    fs::remove_dir_all(path)
}
