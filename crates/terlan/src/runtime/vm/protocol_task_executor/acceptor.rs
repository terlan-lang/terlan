//! Bounded connection acceptance and fixed-owner admission.

use std::io;
use std::net as std_net;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use mio::net::{TcpListener, TcpStream};
use mio::{Interest, Registry, Waker as MioWaker};

use super::{
    render_io, reserve_admission_target, reserve_remote_admission_target, VmProtocolShardIngress,
    ACCEPTOR_LISTENER_TOKEN, MAX_ACCEPTS_PER_TICK,
};

#[derive(Default)]
pub(super) struct VmProtocolCapacity {
    pub(super) waiting: AtomicBool,
    notification_epoch: AtomicUsize,
    wakers: Mutex<Vec<Arc<MioWaker>>>,
}

impl VmProtocolCapacity {
    pub(super) fn register(&self, waker: &Arc<MioWaker>) -> Result<(), String> {
        self.wakers
            .lock()
            .map_err(|_| "error[vm.protocol_capacity]: waker registry poisoned".to_string())?
            .push(Arc::clone(waker));
        Ok(())
    }

    fn park(&self) {
        self.waiting.store(true, Ordering::Release);
    }

    pub(super) fn notification_epoch(&self) -> usize {
        self.notification_epoch.load(Ordering::Acquire)
    }

    pub(super) fn notify(&self) {
        if !self.waiting.load(Ordering::Relaxed) || !self.waiting.swap(false, Ordering::AcqRel) {
            return;
        }
        self.notification_epoch.fetch_add(1, Ordering::Release);
        if let Ok(wakers) = self.wakers.lock() {
            for waker in wakers.iter() {
                let _ = waker.wake();
            }
        }
    }
}

/// Per-owner accept point with VM-directed overload correction.
pub(super) struct VmProtocolAcceptor {
    listener: TcpListener,
    ingresses: Vec<Arc<VmProtocolShardIngress>>,
    local_index: usize,
    capacity: Arc<VmProtocolCapacity>,
    next_tie: usize,
    local_admissions: Vec<TcpStream>,
    pending_admission: Option<TcpStream>,
}

impl VmProtocolAcceptor {
    pub(super) fn new(
        listener: std_net::TcpListener,
        registry: &Registry,
        ingresses: Vec<Arc<VmProtocolShardIngress>>,
        local_index: usize,
        capacity: Arc<VmProtocolCapacity>,
    ) -> Result<Self, String> {
        listener
            .set_nonblocking(true)
            .map_err(render_io("acceptor listener"))?;
        let mut listener = TcpListener::from_std(listener);
        registry
            .register(&mut listener, ACCEPTOR_LISTENER_TOKEN, Interest::READABLE)
            .map_err(render_io("acceptor listener registration"))?;
        Ok(Self {
            listener,
            ingresses,
            local_index,
            capacity,
            next_tie: local_index,
            local_admissions: Vec::with_capacity(64),
            pending_admission: None,
        })
    }

    /// Drains a bounded listener batch and reports whether admission should
    /// continue immediately after the owner services its current tasks.
    pub(super) fn accept_ready(&mut self) -> Result<bool, String> {
        for accepted in 0..MAX_ACCEPTS_PER_TICK {
            let stream = match self.pending_admission.take() {
                Some(stream) => stream,
                None => match self.listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(format!("error[vm.protocol_accept]: {error}")),
                },
            };
            let target =
                match reserve_admission_target(&self.ingresses, self.local_index, self.next_tie) {
                    Some(target) => target,
                    None => {
                        self.capacity.park();
                        match reserve_admission_target(
                            &self.ingresses,
                            self.local_index,
                            self.next_tie,
                        ) {
                            Some(target) => {
                                self.capacity.waiting.store(false, Ordering::Release);
                                target
                            }
                            None => {
                                self.pending_admission = Some(stream);
                                return Ok(false);
                            }
                        }
                    }
                };
            self.admit_reserved(stream, target)?;
            if accepted == 1 {
                self.spill_local_admissions()?;
            }
        }
        Ok(true)
    }

    fn admit_reserved(&mut self, stream: TcpStream, target: usize) -> Result<(), String> {
        if target == self.local_index {
            self.local_admissions.push(stream);
        } else {
            self.ingresses[target].admit_reserved(stream, true)?;
        }
        self.next_tie = (target + 1) % self.ingresses.len();
        Ok(())
    }

    /// Moves the isolated fast-path admission off the acceptor for busy waves.
    fn spill_local_admissions(&mut self) -> Result<(), String> {
        let mut local = std::mem::take(&mut self.local_admissions);
        let mut retained = Vec::with_capacity(local.capacity());
        for stream in local.drain(..) {
            let Some(target) =
                reserve_remote_admission_target(&self.ingresses, self.local_index, self.next_tie)
            else {
                retained.push(stream);
                continue;
            };
            self.ingresses[self.local_index].release_reservation();
            self.ingresses[target].admit_reserved(stream, true)?;
            self.next_tie = (target + 1) % self.ingresses.len();
        }
        self.local_admissions = retained;
        Ok(())
    }

    pub(super) fn take_local_admissions(&mut self) -> Vec<TcpStream> {
        std::mem::take(&mut self.local_admissions)
    }

    pub(super) fn recycle_local_admissions(&mut self, mut admissions: Vec<TcpStream>) {
        admissions.clear();
        self.local_admissions = admissions;
    }
}
