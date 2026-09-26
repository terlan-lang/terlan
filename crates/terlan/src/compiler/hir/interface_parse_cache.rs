//! Content-keyed interface parsing shared by file and embedded consumers.

use std::collections::{hash_map::RandomState, HashMap};
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::ModuleInterface;

type Parsed = Option<(String, ModuleInterface)>;
const PARSE_STRIPES: usize = 64;
static CACHE: OnceLock<InterfaceCache> = OnceLock::new();

struct Budget {
    entries: usize,
    source_bytes: usize,
    idle: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            entries: 4096,
            source_bytes: 32 * 1024 * 1024,
            idle: Duration::from_secs(900),
        }
    }
}

struct Entry {
    parsed: Arc<Parsed>,
    used: Instant,
}

#[derive(Default)]
struct Resident {
    entries: HashMap<Arc<str>, Entry>,
    source_bytes: usize,
}

struct InterfaceCache {
    budget: Budget,
    resident: Mutex<Resident>,
    stripes: [Mutex<()>; PARSE_STRIPES],
    hash: RandomState,
    requests: AtomicU64,
    attempts: AtomicU64,
}

impl InterfaceCache {
    fn new(budget: Budget) -> Self {
        assert!(budget.entries > 0 && budget.source_bytes > 0 && !budget.idle.is_zero());
        Self {
            budget,
            resident: Mutex::new(Resident::default()),
            stripes: std::array::from_fn(|_| Mutex::new(())),
            hash: RandomState::new(),
            requests: AtomicU64::new(0),
            attempts: AtomicU64::new(0),
        }
    }

    /// The parser is pure and non-reentrant. A stripe coordinates identical
    /// bytes without holding the resident-map lock during parsing or cloning.
    fn parse_at(&self, source: &str, now: Instant, parse: impl FnOnce(&str) -> Parsed) -> Parsed {
        self.requests.fetch_add(1, Ordering::Relaxed);
        let stripe = self.hash.hash_one(source) as usize % PARSE_STRIPES;
        // A panicking parser mutates no stripe state and publishes no result.
        // Recovering this unit lock permits a later request to retry safely.
        let stripe = self.stripes[stripe]
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        {
            let mut resident = self.resident.lock().expect("interface cache inventory");
            if let Some(entry) = resident.entries.get_mut(source) {
                if now.saturating_duration_since(entry.used) <= self.budget.idle {
                    entry.used = now;
                    let parsed = Arc::clone(&entry.parsed);
                    drop(resident);
                    drop(stripe);
                    return (*parsed).clone();
                }
            }
        }
        self.attempts.fetch_add(1, Ordering::Relaxed);
        let parsed = Arc::new(parse(source));
        // Cache capacity is not a language limit. Oversized interfaces remain
        // valid but are not retained in this process-local acceleration cache.
        if source.len() <= self.budget.source_bytes {
            let mut resident = self.resident.lock().expect("interface cache inventory");
            let expired = resident
                .entries
                .iter()
                .filter(|(_, entry)| now.saturating_duration_since(entry.used) > self.budget.idle)
                .map(|(key, _)| Arc::clone(key))
                .collect::<Vec<_>>();
            for key in expired {
                remove(&mut resident, &key);
            }
            while resident.entries.len() >= self.budget.entries
                || resident.source_bytes > self.budget.source_bytes - source.len()
            {
                let oldest = resident
                    .entries
                    .iter()
                    .min_by(|(left_key, left), (right_key, right)| {
                        (left.used, left_key.as_ref()).cmp(&(right.used, right_key.as_ref()))
                    })
                    .map(|(key, _)| Arc::clone(key))
                    .expect("nonempty over-budget cache");
                remove(&mut resident, &oldest);
            }
            resident.source_bytes += source.len();
            resident.entries.insert(
                Arc::from(source),
                Entry {
                    parsed: Arc::clone(&parsed),
                    used: now,
                },
            );
        }
        drop(stripe);
        (*parsed).clone()
    }
}

fn remove(resident: &mut Resident, key: &str) {
    if resident.entries.remove(key).is_some() {
        resident.source_bytes -= key.len();
    }
}

/// Parses exact content once while resident, including negative parse results.
/// Paths and timestamps are deliberately not validity evidence; callers reread
/// files, and each changed byte sequence has a distinct cache entry.
pub(crate) fn parse_interface_text(source: &str) -> Parsed {
    CACHE
        .get_or_init(|| InterfaceCache::new(Budget::default()))
        .parse_at(source, Instant::now(), |source| {
            let parsed =
                crate::terlan_syntax::parse_interface_module_as_syntax_output(source).ok()?;
            let module = parsed.module_name.clone();
            Some((module, super::syntax_module_output_to_interface(&parsed)))
        })
}

/// Returns cumulative process-local interface requests and actual parse attempts.
/// Attempts include invalid input and failed/panicking parses, not only success.
pub(crate) fn interface_parse_counts() -> (u64, u64) {
    CACHE.get().map_or((0, 0), |cache| {
        (
            cache.requests.load(Ordering::Relaxed),
            cache.attempts.load(Ordering::Relaxed),
        )
    })
}

#[cfg(test)]
#[path = "interface_parse_cache_test.rs"]
mod tests;
