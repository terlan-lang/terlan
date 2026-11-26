use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// One bounded progress observation emitted by a native planning operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgressEvent {
    pub operation: String,
    pub phase: String,
    pub elapsed_ms: u128,
    pub resident_bytes: Option<u64>,
}

/// Background progress reporter stopped and joined automatically on drop.
pub struct ProgressWatch {
    shared: Arc<(Mutex<ProgressState>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

struct ProgressState {
    phase: String,
    stopped: bool,
}

impl ProgressWatch {
    /// Starts a reporter that emits the latest phase at each positive interval.
    pub fn spawn(
        operation: impl Into<String>,
        phase: impl Into<String>,
        interval: Duration,
        mut emit: impl FnMut(ProgressEvent) + Send + 'static,
    ) -> Self {
        assert!(!interval.is_zero(), "progress interval must be positive");
        let operation = operation.into();
        let shared = Arc::new((
            Mutex::new(ProgressState {
                phase: phase.into(),
                stopped: false,
            }),
            Condvar::new(),
        ));
        let observed = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name("terlan-planning-progress".into())
            .spawn(move || {
                let started = Instant::now();
                let (state, changed) = &*observed;
                loop {
                    let current = state.lock().expect("progress state lock");
                    let (current, _) = changed
                        .wait_timeout_while(current, interval, |value| !value.stopped)
                        .expect("progress wait");
                    if current.stopped {
                        return;
                    }
                    let event = ProgressEvent {
                        operation: operation.clone(),
                        phase: current.phase.clone(),
                        elapsed_ms: started.elapsed().as_millis(),
                        resident_bytes: current_resident_bytes(),
                    };
                    drop(current);
                    emit(event);
                }
            })
            .expect("progress worker must start");
        Self {
            shared,
            worker: Some(worker),
        }
    }

    /// Replaces the phase attached to subsequent progress observations.
    pub fn phase(&self, phase: impl Into<String>) {
        self.shared.0.lock().expect("progress state lock").phase = phase.into();
    }
}

#[cfg(target_os = "linux")]
fn current_resident_bytes() -> Option<u64> {
    resident_bytes_from_path(std::path::Path::new("/proc/self/status"))
}

#[cfg(target_os = "linux")]
fn resident_bytes_from_path(path: &std::path::Path) -> Option<u64> {
    let status = std::fs::read_to_string(path).ok()?;
    resident_bytes_from_status(&status)
}

#[cfg(not(target_os = "linux"))]
fn current_resident_bytes() -> Option<u64> {
    None
}

fn resident_bytes_from_status(status: &str) -> Option<u64> {
    let mut fields = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace();
    let kibibytes = fields.next()?.parse::<u64>().ok()?;
    if fields.next()? != "kB" || fields.next().is_some() {
        return None;
    }
    kibibytes.checked_mul(1024)
}

impl Drop for ProgressWatch {
    fn drop(&mut self) {
        {
            let mut state = self.shared.0.lock().expect("progress state lock");
            state.stopped = true;
        }
        self.shared.1.notify_one();
        if let Some(worker) = self.worker.take() {
            worker.join().expect("progress worker must stop");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn reports_the_latest_phase_and_stops() {
        let (sender, receiver) = mpsc::channel();
        let watch = ProgressWatch::spawn(
            "native-image",
            "frontend",
            Duration::from_millis(5),
            move |event| sender.send(event).expect("progress receiver"),
        );
        watch.phase("reachability");
        let event = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("progress event");
        assert_eq!(event.operation, "native-image");
        assert_eq!(event.phase, "reachability");
        assert!(event.elapsed_ms > 0);
        assert!(event.resident_bytes.is_some_and(|bytes| bytes > 0));
        drop(watch);
        assert!(receiver.recv_timeout(Duration::from_millis(20)).is_err());
    }

    #[test]
    #[should_panic(expected = "progress interval must be positive")]
    fn rejects_a_zero_interval() {
        let _watch = ProgressWatch::spawn(
            "native-image",
            "start",
            Duration::ZERO,
            std::mem::drop::<ProgressEvent>,
        );
    }

    #[test]
    fn dropping_an_already_joined_watch_is_safe() {
        let mut watch = ProgressWatch::spawn(
            "native-image",
            "start",
            Duration::from_secs(1),
            std::mem::drop::<ProgressEvent>,
        );
        {
            let mut state = watch.shared.0.lock().expect("progress state");
            state.stopped = true;
        }
        watch.shared.1.notify_one();
        watch
            .worker
            .take()
            .expect("progress worker")
            .join()
            .expect("joined progress worker");
        drop(watch);
    }

    #[test]
    fn parses_linux_resident_memory_without_guessing_units() {
        assert_eq!(
            resident_bytes_from_status("Name:\tterlc\nVmRSS:\t1234 kB\n"),
            Some(1_263_616)
        );
        for status in [
            "Name:\tterlc\n",
            "VmRSS:\n",
            "VmRSS:\t1\n",
            "VmRSS:\tinvalid kB\n",
            "VmRSS:\t1 MB\n",
            "VmRSS:\t1 kB trailing\n",
            "VmRSS:\t18014398509481984 kB\n",
        ] {
            assert_eq!(resident_bytes_from_status(status), None);
        }
        assert_eq!(
            resident_bytes_from_path(std::path::Path::new("/missing/terlan/status")),
            None
        );
    }
}
