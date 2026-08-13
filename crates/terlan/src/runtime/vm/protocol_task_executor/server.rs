//! Supervisor handle for one fixed-scheduler protocol service.

use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;

use super::{VmProtocolControl, VmProtocolControlPort};

/// One ready protocol listener and all fixed scheduler threads that own it.
pub(crate) struct VmProtocolTaskServer {
    #[cfg(test)]
    address: SocketAddr,
    controls: Vec<Arc<VmProtocolControlPort>>,
    threads: Vec<thread::JoinHandle<Result<(), String>>>,
    stopped: bool,
}

impl VmProtocolTaskServer {
    /// Creates a ready supervisor handle after every owner reports readiness.
    pub(super) fn new(
        _address: SocketAddr,
        controls: Vec<Arc<VmProtocolControlPort>>,
        threads: Vec<thread::JoinHandle<Result<(), String>>>,
    ) -> Self {
        Self {
            #[cfg(test)]
            address: _address,
            controls,
            threads,
            stopped: false,
        }
    }

    /// Returns the exact bound listener address advertised by this service.
    #[cfg(test)]
    pub(crate) const fn local_addr(&self) -> SocketAddr {
        self.address
    }

    /// Stops admission, drops connection futures, and joins every scheduler.
    pub(crate) fn stop(&mut self) -> Result<(), String> {
        if self.stopped {
            return Ok(());
        }
        self.stopped = true;
        stop_protocol_threads(&self.controls);
        join_protocol_threads(&mut self.threads)
    }

    /// Waits until every protocol scheduler exits or one reports failure.
    pub(crate) fn join(mut self) -> Result<(), String> {
        let result = join_protocol_threads(&mut self.threads);
        self.stopped = true;
        result
    }
}

impl Drop for VmProtocolTaskServer {
    /// Prevents a dropped supervisor from leaving detached protocol owners.
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Publishes one shutdown command to every fixed protocol owner.
pub(super) fn stop_protocol_threads(controls: &[Arc<VmProtocolControlPort>]) {
    for control in controls {
        let _ = control.messages.push(VmProtocolControl::Shutdown);
        let _ = control.poll_waker.wake();
    }
}

/// Joins every protocol owner and returns the first deterministic failure.
pub(super) fn join_protocol_threads(
    threads: &mut Vec<thread::JoinHandle<Result<(), String>>>,
) -> Result<(), String> {
    let mut first_error = None;
    for thread in threads.drain(..) {
        let result = thread.join().map_err(|_| {
            "error[vm.protocol_scheduler]: fixed protocol scheduler panicked".to_string()
        });
        let result = result.and_then(|result| result);
        if first_error.is_none() {
            first_error = result.err();
        }
    }
    first_error.map_or(Ok(()), Err)
}
