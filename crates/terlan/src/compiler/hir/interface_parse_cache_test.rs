//! Content identity, bounded retention, and concurrent parse ownership.

use super::*;
use std::fs;
use std::sync::{mpsc, Barrier};

fn cache(entries: usize, source_bytes: usize) -> InterfaceCache {
    InterfaceCache::new(Budget {
        entries,
        source_bytes,
        idle: Duration::from_secs(10),
    })
}

fn parsed(source: &str) -> Parsed {
    Some((
        source.into(),
        ModuleInterface {
            module: source.into(),
            ..ModuleInterface::default()
        },
    ))
}

#[test]
fn exact_content_reuses_parsing_and_returns_independent_values() {
    let cache = cache(4, 64);
    let now = Instant::now();
    let mut first = cache.parse_at("same", now, parsed).unwrap();
    first.1.module = "caller mutation".into();
    let second = cache
        .parse_at("same", now, |_| panic!("duplicate parse"))
        .unwrap();
    assert_eq!(second.1.module, "same");
    assert_eq!(cache.attempts.load(Ordering::Relaxed), 1);
    cache.parse_at("different", now, parsed).unwrap();
    assert_eq!(cache.attempts.load(Ordering::Relaxed), 2);
}

#[test]
fn invalid_content_is_cached_but_a_changed_input_is_reconsidered() {
    let cache = cache(4, 64);
    let now = Instant::now();
    assert!(cache.parse_at("invalid", now, |_| None).is_none());
    assert!(cache
        .parse_at("invalid", now, |_| panic!("duplicate failure parse"))
        .is_none());
    assert!(cache.parse_at("valid", now, parsed).is_some());
    assert_eq!(cache.attempts.load(Ordering::Relaxed), 2);
}

#[test]
fn concurrent_identical_requests_invoke_one_parser() {
    let cache = cache(4, 64);
    let barrier = Barrier::new(8);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                barrier.wait();
                let value = cache
                    .parse_at("shared", Instant::now(), |source| {
                        std::thread::sleep(Duration::from_millis(20));
                        parsed(source)
                    })
                    .unwrap();
                assert_eq!(value.0, "shared");
            });
        }
    });
    assert_eq!(cache.requests.load(Ordering::Relaxed), 8);
    assert_eq!(cache.attempts.load(Ordering::Relaxed), 1);
}

#[test]
fn unrelated_stripes_parse_without_a_global_parser_lock() {
    let cache = cache(4, 64);
    let first = "first";
    let first_stripe = cache.hash.hash_one(first) as usize % PARSE_STRIPES;
    let second = (0..1024)
        .map(|index| format!("other-{index}"))
        .find(|value| cache.hash.hash_one(value.as_str()) as usize % PARSE_STRIPES != first_stripe)
        .unwrap();
    let (started, observed) = mpsc::channel();
    let (release_first, first_released) = mpsc::channel();
    let (release_second, second_released) = mpsc::channel();
    std::thread::scope(|scope| {
        let first_started = started.clone();
        let cache_ref = &cache;
        scope.spawn(move || {
            cache_ref.parse_at(first, Instant::now(), |source| {
                first_started.send(()).unwrap();
                first_released.recv_timeout(Duration::from_secs(5)).unwrap();
                parsed(source)
            })
        });
        scope.spawn(move || {
            cache_ref.parse_at(&second, Instant::now(), |source| {
                started.send(()).unwrap();
                second_released
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
                parsed(source)
            })
        });
        let first_started = observed.recv_timeout(Duration::from_secs(2));
        let second_started = observed.recv_timeout(Duration::from_secs(2));
        release_first.send(()).unwrap();
        release_second.send(()).unwrap();
        assert!(first_started.is_ok() && second_started.is_ok());
    });
}

#[test]
fn entry_pressure_evicts_the_least_recently_used_result() {
    let cache = cache(2, 64);
    let now = Instant::now();
    cache.parse_at("first", now, parsed);
    cache.parse_at("second", now + Duration::from_secs(1), parsed);
    cache.parse_at("first", now + Duration::from_secs(2), parsed);
    cache.parse_at("third", now + Duration::from_secs(3), parsed);
    let resident = cache.resident.lock().unwrap();
    assert_eq!(resident.entries.len(), 2);
    assert!(resident.entries.contains_key("first"));
    assert!(!resident.entries.contains_key("second"));
    assert_eq!(resident.source_bytes, "firstthird".len());
}

#[test]
fn byte_pressure_is_bounded_and_oversized_inputs_still_parse() {
    let cache = cache(4, 5);
    let now = Instant::now();
    cache.parse_at("aaa", now, parsed);
    cache.parse_at("bbbb", now, parsed);
    assert_eq!(
        cache.parse_at("oversized", now, parsed).unwrap().0,
        "oversized"
    );
    let resident = cache.resident.lock().unwrap();
    assert_eq!(resident.entries.len(), 1);
    assert!(resident.entries.contains_key("bbbb"));
    assert_eq!(resident.source_bytes, 4);
}

#[test]
fn idle_expiry_releases_old_source_bytes() {
    let cache = cache(4, 64);
    let now = Instant::now();
    cache.parse_at("old", now, parsed);
    cache.parse_at("new", now + Duration::from_secs(11), parsed);
    let resident = cache.resident.lock().unwrap();
    assert_eq!(resident.entries.len(), 1);
    assert_eq!(resident.source_bytes, 3);
}

#[test]
fn a_panicking_parser_does_not_poison_future_requests() {
    let cache = cache(4, 64);
    let now = Instant::now();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cache.parse_at("retry", now, |_| panic!("injected parse failure"))
    }))
    .is_err());
    assert_eq!(cache.parse_at("retry", now, parsed).unwrap().0, "retry");
    assert_eq!(cache.attempts.load(Ordering::Relaxed), 2);
}

#[test]
fn same_size_same_timestamp_edits_invalidate_real_file_interfaces() {
    let root = crate::support::test_fs::TestDirectory::new("interface_parse_cache", "editor_edit");
    let path = root.join("Interface.typi");
    let original = "module cache_edit.Interface.\n\npub first(): Int.\n";
    let changed = "module cache_edit.Interface.\n\npub other(): Int.\n";
    assert_eq!(original.len(), changed.len());
    fs::write(&path, original).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let first = super::super::parse_interface_file(&path).unwrap().1;
    assert!(first.functions.contains_key(&("first".into(), 0)));
    fs::write(&path, changed).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let second = super::super::parse_interface_file(&path).unwrap().1;
    assert!(second.functions.contains_key(&("other".into(), 0)));
    assert!(!second.functions.contains_key(&("first".into(), 0)));
    fs::write(&path, "malformed interface").unwrap();
    assert!(super::super::parse_interface_file(&path).is_none());
    fs::write(&path, original).unwrap();
    assert!(super::super::parse_interface_file(&path)
        .unwrap()
        .1
        .functions
        .contains_key(&("first".into(), 0)));
    fs::remove_file(&path).unwrap();
    assert!(super::super::parse_interface_file(&path).is_none());
    root.close();
}
