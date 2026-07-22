use super::super::process::VmProcessId;
use super::{VmPackageDownloadEvent, VmPackageDownloadRuntime, VmPackageDownloadWake};

/// Verifies package download chunk readiness, wakeups, completion, and
/// inspection.
///
/// Inputs:
/// - One package download handle, a parked actor, chunks, and a completion
///   event.
///
/// Output:
/// - Test passes when chunks preserve order, wake blocked receivers, and expose
///   pressure through inspection.
///
/// Transformation:
/// - Exercises package transport scheduling without a host async runtime or
///   hand-rolled HTTP client.
#[test]
fn package_download_transport_parks_and_wakes_when_chunk_arrives() {
    let mut runtime = VmPackageDownloadRuntime::new();
    let download = runtime
        .start_download("https://packages.terlan.test/pkg", "resolver", 4)
        .expect("start download");
    let process = VmProcessId::from_raw_for_test(900);

    assert!(runtime
        .park_receive(download, process)
        .expect("park receive"));
    let wakeups = runtime
        .enqueue_chunk(download, b"abc".to_vec())
        .expect("enqueue chunk");
    assert_eq!(
        wakeups,
        vec![VmPackageDownloadWake::Chunk { process, download }]
    );
    runtime
        .enqueue_chunk(download, b"def".to_vec())
        .expect("enqueue second chunk");

    let info = runtime.inspect_download(download).expect("inspect");
    assert_eq!(info.queued_chunks, 2);
    assert_eq!(info.queued_bytes, 6);
    assert_eq!(info.waiting_receivers, 0);

    assert_eq!(
        runtime.receive_next(download).expect("receive first"),
        Some(VmPackageDownloadEvent::Chunk(b"abc".to_vec()))
    );
    assert_eq!(
        runtime.receive_next(download).expect("receive second"),
        Some(VmPackageDownloadEvent::Chunk(b"def".to_vec()))
    );
    assert_eq!(runtime.receive_next(download).expect("empty"), None);

    assert!(runtime
        .park_receive(download, process)
        .expect("park for completion"));
    assert_eq!(
        runtime.finish_download(download).expect("finish"),
        vec![VmPackageDownloadWake::Complete { process, download }]
    );
    assert_eq!(
        runtime.receive_next(download).expect("complete event"),
        Some(VmPackageDownloadEvent::Complete)
    );
    assert_eq!(
        runtime.receive_next(download).expect("complete spent"),
        None
    );
}

/// Verifies package download backpressure and actor-owned cancellation.
///
/// Inputs:
/// - One bounded package download and a cancellation through owner cleanup.
///
/// Output:
/// - Test passes when the queue bound rejects excess chunks and cancellation
///   makes the handle stale.
///
/// Transformation:
/// - Locks the resource behavior package downloads need before package
///   resolver/network integration is wired into the VM.
#[test]
fn package_download_transport_enforces_backpressure_and_cancels_owner() {
    let mut runtime = VmPackageDownloadRuntime::new();
    assert_eq!(
        runtime
            .start_download("", "resolver", 1)
            .expect_err("empty url should fail"),
        "VM package download URL cannot be empty"
    );
    assert_eq!(
        runtime
            .start_download("https://packages.terlan.test/pkg", "resolver", 0)
            .expect_err("zero queue should fail"),
        "VM package download queue limit must be greater than 0"
    );
    let download = runtime
        .start_download("https://packages.terlan.test/pkg", "resolver", 1)
        .expect("start download");

    runtime
        .enqueue_chunk(download, b"abc".to_vec())
        .expect("first chunk");
    assert_eq!(
        runtime
            .enqueue_chunk(download, b"def".to_vec())
            .expect_err("queue full"),
        "VM package download chunk queue is full"
    );

    let cancelled = runtime.cancel_owner_downloads("resolver");
    assert_eq!(cancelled, vec![download]);
    assert_eq!(
        runtime
            .receive_next(download)
            .expect_err("cancelled download is stale"),
        "VM package download is cancelled"
    );
    assert_eq!(
        runtime
            .enqueue_chunk(download, b"after cancel".to_vec())
            .expect_err("cancelled enqueue is stale"),
        "VM package download is cancelled"
    );
}
