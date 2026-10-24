use super::*;
use crate::session_test_support::{failure, Probe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};

#[test]
fn concurrent_images_initialize_one_application_once() {
    let applications = Arc::new(ApplicationSessions::default());
    let start = Arc::new(Barrier::new(16));
    let creations = Arc::new(AtomicUsize::new(0));
    let workers = (0..16)
        .map(|_| {
            let applications = Arc::clone(&applications);
            let start = Arc::clone(&start);
            let creations = Arc::clone(&creations);
            std::thread::spawn(move || {
                start.wait();
                applications
                    .get_or_create("app", || {
                        creations.fetch_add(1, Ordering::SeqCst);
                        Probe::default()
                    })
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    let services = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(creations.load(Ordering::SeqCst), 1);
    services[0]
        .with_storage(|state| state.value = Some("shared".into()))
        .unwrap();
    for service in services {
        service
            .with_storage(|state| {
                assert_eq!(state.calls, [vec!["start_clock"]]);
                assert_eq!(state.value.as_deref(), Some("shared"));
            })
            .unwrap();
    }
    let reloaded = applications
        .get_or_create("app", || panic!("reload must reuse storage"))
        .unwrap();
    drop(applications);
    // A running image retains its context even when the registry is dropped.
    reloaded
        .with_storage(|state| assert_eq!(state.value.as_deref(), Some("shared")))
        .unwrap();
}

#[test]
fn separate_keys_are_isolated_and_failed_clock_attachment_can_be_retried() {
    let applications = ApplicationSessions::default();
    assert_eq!(
        applications
            .get_or_create("first", || Probe {
                fail: true,
                ..Probe::default()
            })
            .unwrap_err(),
        failure()
    );
    assert!(applications.services.lock().unwrap().is_empty());
    let first = applications.get_or_create("first", Probe::default).unwrap();
    let second = applications
        .get_or_create("second", Probe::default)
        .unwrap();
    first
        .with_storage(|state| state.value = Some("private".into()))
        .unwrap();
    second
        .with_storage(|state| {
            assert_eq!(state.value, None);
            assert_eq!(state.calls, [vec!["start_clock"]]);
        })
        .unwrap();
}

#[test]
fn poisoned_application_registry_fails_closed_without_creating_storage() {
    let applications = Arc::new(ApplicationSessions::<&str, Probe>::default());
    let clone = Arc::clone(&applications);
    assert!(std::thread::spawn(move || {
        let _guard = clone.services.lock().unwrap();
        panic!("poison fixture");
    })
    .join()
    .is_err());
    let error = applications
        .get_or_create("app", || panic!("must not create after poisoning"))
        .unwrap_err();
    assert!(error.to_string().contains("error[serve.session_cache]"));
}
