use super::*;
use crate::session_test_support::{failure, Probe};
use std::time::Duration;

impl SessionHost for Probe {
    fn start_clock(&mut self) -> Result<(), BoundaryError> {
        self.record(&["start_clock"])
    }

    fn maintain(&mut self, limit: usize) -> Result<(), BoundaryError> {
        self.record(&["maintain", &limit.to_string()])
    }
}

#[test]
fn session_images_share_the_application_context_but_not_other_applications() {
    let service = SessionService::new(Probe::default());
    let clone = service.clone();
    let old_image = service.native_services().unwrap();
    let new_image = clone.native_services().unwrap();
    let other = SessionService::new(Probe::default())
        .native_services()
        .unwrap();
    service
        .with_storage(|state| state.value = Some("stored".into()))
        .unwrap();
    assert!(format!("{service:?}").contains("SessionService"));
    drop(service);
    drop(clone);
    for image in [old_image, new_image] {
        assert_eq!(
            image.call("std.http.session.get", &["session".into(), "key".into()]),
            Ok(Some("stored").into())
        );
        assert!(image.call("std.http.session.cookie", &[]).is_err());
    }
    assert_eq!(
        other.call("std.http.session.get", &["session".into(), "key".into()]),
        Ok(None::<String>.into())
    );
}

#[test]
fn session_maintenance_does_not_wait_for_a_request_lock_and_retries_later() {
    let service = SessionService::new(Probe::default());
    let guard = service.storage.lock().unwrap();
    let clone = service.clone();
    let (send, receive) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || send.send(clone.maintain(7)).unwrap());
    let result = receive.recv_timeout(Duration::from_secs(5));
    assert!(guard.calls.is_empty());
    // Always release and join before asserting the timeout so regressions cannot
    // leave a blocked worker behind.
    drop(guard);
    worker.join().unwrap();
    assert_eq!(result.unwrap(), Ok(()));
    service.start_clock().unwrap();
    service.maintain(7).unwrap();
    service
        .with_storage(|state| {
            assert_eq!(state.calls, [vec!["start_clock"], vec!["maintain", "7"]]);
        })
        .unwrap();
}

#[test]
fn session_host_failures_are_typed_and_retried_without_poisoning_the_context() {
    let service = SessionService::new(Probe {
        fail: true,
        ..Probe::default()
    });
    assert_eq!(service.start_clock(), Err(failure()));
    assert_eq!(service.maintain(0), Err(failure()));
    service.with_storage(|state| state.fail = false).unwrap();
    service.start_clock().unwrap();
    service.maintain(0).unwrap();
    service
        .with_storage(|state| {
            assert_eq!(state.calls.len(), 4);
            assert_eq!(state.calls[1], ["maintain", "0"]);
            assert_eq!(state.calls[3], ["maintain", "0"]);
        })
        .unwrap();
}

#[test]
fn session_poisoned_contexts_fail_closed_for_host_and_image_access() {
    let service = SessionService::new(Probe::default());
    let image = service.native_services().unwrap();
    let clone = service.clone();
    assert!(
        std::thread::spawn(move || { clone.with_storage(|_| panic!("poison fixture")) })
            .join()
            .is_err()
    );
    for result in [
        service.start_clock(),
        service.maintain(1),
        service.with_storage(|_| ()),
    ] {
        assert!(result.unwrap_err().to_string().contains("lock poisoned"));
    }
    assert!(image
        .call("std.http.session.create", &["".into(), 10_i64.into()])
        .is_err());
}
