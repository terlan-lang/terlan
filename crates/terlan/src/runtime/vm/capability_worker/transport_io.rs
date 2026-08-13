use super::*;

/// Serializes queued requests outside VM scheduler threads.
pub(super) fn run_writer(
    mut input: impl Write,
    requests: Receiver<CapabilityRequest>,
    events: SyncSender<VmCapabilityWorkerTransportEvent>,
    event_wakers: Arc<Mutex<Vec<Waker>>>,
    max_payload_bytes: usize,
) {
    while let Ok(request) = requests.recv() {
        if let Err(error) = write_json_frame(&mut input, &request, max_payload_bytes) {
            publish_transport_event(
                &events,
                &event_wakers,
                VmCapabilityWorkerTransportEvent::Failed(error),
            );
            break;
        }
    }
}

/// Decodes worker replies outside VM scheduler threads.
pub(super) fn run_reader(
    output: impl Read,
    events: SyncSender<VmCapabilityWorkerTransportEvent>,
    event_wakers: Arc<Mutex<Vec<Waker>>>,
    max_payload_bytes: usize,
) {
    let mut output = BufReader::new(output);
    loop {
        match read_json_frame(&mut output, max_payload_bytes) {
            Ok(Some(response)) => {
                if !publish_transport_event(
                    &events,
                    &event_wakers,
                    VmCapabilityWorkerTransportEvent::Response(response),
                ) {
                    break;
                }
            }
            Ok(None) => {
                publish_transport_event(
                    &events,
                    &event_wakers,
                    VmCapabilityWorkerTransportEvent::Closed,
                );
                break;
            }
            Err(error) => {
                publish_transport_event(
                    &events,
                    &event_wakers,
                    VmCapabilityWorkerTransportEvent::Failed(error),
                );
                break;
            }
        }
    }
}

pub(super) fn publish_transport_event(
    events: &SyncSender<VmCapabilityWorkerTransportEvent>,
    event_wakers: &Arc<Mutex<Vec<Waker>>>,
    event: VmCapabilityWorkerTransportEvent,
) -> bool {
    if events.send(event).is_err() {
        return false;
    }
    let wakers = event_wakers
        .lock()
        .map(|mut wakers| std::mem::take(&mut *wakers))
        .unwrap_or_default();
    for waker in wakers {
        waker.wake();
    }
    true
}

/// Terminates and reaps one child without panicking during cleanup.
pub(super) fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
