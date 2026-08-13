//! Protocol-agnostic VM ownership for nonblocking socket task futures.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::io;
use std::net::{self as std_net, SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, Mutex, OnceLock, Weak};
#[cfg(test)]
use std::task::Wake;
use std::task::{Context, Poll as TaskPoll};
use std::thread;

use mio::net::TcpStream;
use mio::{Events, Interest, Poll, Token, Waker as MioWaker};
use socket2::{Domain, Protocol, Socket, Type};

use super::scheduler_topology::{VmSchedulerId, VmSchedulerTopology};

mod acceptor;
mod admission;
mod lazy_queue;
mod local_resources;
mod process_ids;
mod server;
mod timers;
mod transport;
mod wake;

use acceptor::{VmProtocolAcceptor, VmProtocolCapacity};
#[cfg(test)]
use admission::{admission_target, least_loaded_shard, sampled_loaded_shard};
use admission::{
    reserve_admission_target, reserve_remote_admission_target, VmProtocolOwnerParked,
    VmProtocolShardLoad,
};
use lazy_queue::VmLazyBoundedQueue;
use local_resources::{
    has_owner_local_scheduled, pop_owner_local_scheduled, push_owner_local_scheduled,
    VmProtocolLocalResources,
};
use process_ids::VmProtocolProcessIds;
pub(crate) use server::VmProtocolTaskServer;
use server::{join_protocol_threads, stop_protocol_threads};
pub(crate) use timers::protocol_sleep_until;
use timers::{next_protocol_timer_timeout, wake_due_protocol_timers};
use transport::{render_io, VmReadyEvent};
pub(crate) use transport::{VmProtocolTaskRoute, VmReadyTcpStream, VmSocketReadinessWake};
#[cfg(test)]
use wake::VmProtocolTaskWake;
use wake::{VmProtocolOwnerWake, VmProtocolTaskWakeSlot};

const ACCEPTOR_LISTENER_TOKEN: Token = Token(0);
const ACCEPTOR_CAPACITY_TOKEN: Token = Token(1);
const TASK_WAKE_TOKEN: Token = Token(2);
const FIRST_TASK_TOKEN: usize = 3;
const MAX_TASKS_PER_SHARD: usize = 4_096;
const MAX_ACCEPTS_PER_TICK: usize = 32;
const MAX_TASK_POLLS_PER_TICK: usize = 1_024;
const MAX_CONTROL_MESSAGES_PER_SHARD: usize = 1_024;
const VM_PROTOCOL_SHARD_STACK_BYTES: usize = 1024 * 1024;

static PROTOCOL_CONTROL_PORTS: OnceLock<Mutex<Vec<Weak<VmProtocolControlPort>>>> = OnceLock::new();
thread_local! {
    static CURRENT_PROTOCOL_SCHEDULER: Cell<Option<VmSchedulerId>> = const { Cell::new(None) };
    static CURRENT_PROTOCOL_TASK: Cell<Option<VmProtocolTaskRoute>> = const { Cell::new(None) };
    static PROTOCOL_LOCAL_RESOURCES: RefCell<VmProtocolLocalResources> =
        RefCell::new(VmProtocolLocalResources::default());
}
pub(crate) type VmProtocolTaskFuture = Pin<Box<dyn Future<Output = Result<(), String>> + 'static>>;
pub(crate) type VmProtocolTaskFactory = Arc<
    dyn Fn(VmReadyTcpStream, VmProtocolTaskRoute) -> VmProtocolTaskFuture + Send + Sync + 'static,
>;

/// Returns the fixed VM scheduler that owns the calling socket-task loop.
pub(crate) fn current_protocol_scheduler() -> Option<VmSchedulerId> {
    CURRENT_PROTOCOL_SCHEDULER.with(Cell::get)
}

/// Mutates one typed resource that exists only on the calling VM owner.
pub(crate) fn with_current_protocol_resource<T: 'static, R>(
    identity: u64,
    initialize: impl FnOnce(VmSchedulerId) -> Result<T, String>,
    use_resource: impl FnOnce(&mut T) -> Result<R, String>,
) -> Result<Option<R>, String> {
    let Some(scheduler) = current_protocol_scheduler() else {
        return Ok(None);
    };
    PROTOCOL_LOCAL_RESOURCES.with(|resources| {
        let mut resources = resources.try_borrow_mut().map_err(|_| {
            "error[vm.protocol_resource]: reentrant shard-local resource access".to_string()
        })?;
        resources
            .with_resource(identity, || initialize(scheduler), use_resource)
            .map(Some)
    })
}

/// Mutates an already-admitted resource on its exact protocol owner.
///
/// Continuation resumption must never recreate an empty execution shard after
/// reload or retirement because doing so would silently lose actor state.
pub(crate) fn with_existing_current_protocol_resource<T: 'static, R>(
    identity: u64,
    use_resource: impl FnOnce(&mut T) -> Result<R, String>,
) -> Result<R, String> {
    current_protocol_scheduler().ok_or_else(|| {
        "error[vm.protocol_resource_owner]: operation is outside a protocol owner".to_string()
    })?;
    PROTOCOL_LOCAL_RESOURCES.with(|resources| {
        resources
            .try_borrow_mut()
            .map_err(|_| {
                "error[vm.protocol_resource]: reentrant shard-local resource access".to_string()
            })?
            .with_existing_resource(identity, use_resource)
    })
}

/// Retires one generation-qualified resource from every live protocol owner.
pub(crate) fn retire_protocol_resource(identity: u64) -> Result<(), String> {
    let ports = PROTOCOL_CONTROL_PORTS.get_or_init(|| Mutex::new(Vec::new()));
    let mut ports = ports
        .lock()
        .map_err(|_| "error[vm.protocol_control]: registry lock poisoned".to_string())?;
    let mut failure = None;
    ports.retain(|port| {
        let Some(port) = port.upgrade() else {
            return false;
        };
        if port
            .messages
            .push(VmProtocolControl::RetireResource(identity))
            .is_err()
        {
            failure = Some(format!(
                "error[vm.protocol_control]: scheduler {} control queue full",
                port.scheduler.index()
            ));
        } else if let Err(error) = port.poll_waker.wake() {
            failure = Some(format!(
                "error[vm.protocol_control]: wake scheduler {}: {error}",
                port.scheduler.index()
            ));
        }
        true
    });
    failure.map_or(Ok(()), Err)
}

/// Returns the connection task currently being polled by this protocol owner.
pub(crate) fn current_protocol_task_route() -> Option<VmProtocolTaskRoute> {
    CURRENT_PROTOCOL_TASK.with(Cell::get)
}

#[cfg(test)]
pub(crate) fn with_protocol_scheduler_for_test<R>(
    scheduler: VmSchedulerId,
    operation: impl FnOnce() -> R,
) -> R {
    struct ProtocolSchedulerReset(Option<VmSchedulerId>);
    impl Drop for ProtocolSchedulerReset {
        fn drop(&mut self) {
            CURRENT_PROTOCOL_SCHEDULER.with(|current| current.set(self.0));
        }
    }

    let prior = CURRENT_PROTOCOL_SCHEDULER.with(|current| current.replace(Some(scheduler)));
    let _reset = ProtocolSchedulerReset(prior);
    operation()
}

#[cfg(test)]
/// Runs one test operation under an exact protocol connection owner.
pub(crate) fn with_protocol_task_for_test<R>(
    route: VmProtocolTaskRoute,
    operation: impl FnOnce() -> R,
) -> R {
    with_protocol_scheduler_for_test(route.scheduler(), || with_protocol_task(route, operation))
}

/// Binds one exact connection route only while its future is being polled.
fn with_protocol_task<R>(route: VmProtocolTaskRoute, operation: impl FnOnce() -> R) -> R {
    struct ProtocolTaskReset(Option<VmProtocolTaskRoute>);
    impl Drop for ProtocolTaskReset {
        fn drop(&mut self) {
            CURRENT_PROTOCOL_TASK.with(|current| current.set(self.0));
        }
    }

    let prior = CURRENT_PROTOCOL_TASK.with(|current| current.replace(Some(route)));
    let _reset = ProtocolTaskReset(prior);
    operation()
}

/// Binds a reusable listener before fixed VM protocol shards start.
pub(crate) fn bind_protocol_listener(
    host: &str,
    port: u16,
) -> Result<std_net::TcpListener, String> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|error| format!("error[vm.protocol_bind]: resolve {host}:{port}: {error}"))?;
    let mut last_error = None;
    for address in addresses {
        match bind_address(address) {
            Ok(listener) => return Ok(listener),
            Err(error) => last_error = Some(error),
        }
    }
    Err(format!(
        "error[vm.protocol_bind]: bind {host}:{port}: {}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "host resolved to no addresses".to_string())
    ))
}

/// Runs generic protocol futures only on VM-owned fixed scheduler threads.
pub(crate) fn serve_protocol_tasks(
    listener: std_net::TcpListener,
    factory: VmProtocolTaskFactory,
) -> Result<(), String> {
    let topology = VmSchedulerTopology::from_environment()?;
    start_protocol_tasks_with_topology(listener, factory, topology)?.join()
}

/// Starts a managed protocol server on one explicit fixed-scheduler topology.
pub(crate) fn start_protocol_tasks_with_topology(
    listener: std_net::TcpListener,
    factory: VmProtocolTaskFactory,
    topology: VmSchedulerTopology,
) -> Result<VmProtocolTaskServer, String> {
    let address = listener
        .local_addr()
        .map_err(render_io("listener address"))?;
    let capacity = Arc::new(VmProtocolCapacity::default());
    let mut shards = Vec::with_capacity(topology.width());
    for scheduler in topology.schedulers() {
        shards.push(VmProtocolShardStartup::new(
            scheduler,
            Arc::clone(&factory),
            Arc::clone(&capacity),
        )?);
    }
    let ingresses = shards
        .iter()
        .map(VmProtocolShardStartup::ingress)
        .collect::<Vec<_>>();
    shards[0].attach_acceptor(listener, ingresses, 0, Arc::clone(&capacity))?;
    let controls = shards
        .iter()
        .map(VmProtocolShardStartup::control_port)
        .collect::<Vec<_>>();
    let mut threads = Vec::with_capacity(topology.width());
    for shard in shards {
        let scheduler = shard.scheduler;
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let spawned = thread::Builder::new()
            .name(format!("terlan-vm-protocol-{}", scheduler.index()))
            .stack_size(VM_PROTOCOL_SHARD_STACK_BYTES)
            .spawn(move || {
                CURRENT_PROTOCOL_SCHEDULER.with(|current| current.set(Some(scheduler)));
                let _ = ready_tx.send(());
                shard.run()
            })
            .map_err(|error| format!("error[vm.protocol_scheduler]: {error}"))?;
        threads.push(spawned);
        if ready_rx.recv().is_err() {
            stop_protocol_threads(&controls);
            join_protocol_threads(&mut threads)?;
            return Err(format!(
                "error[vm.protocol_scheduler]: scheduler {} stopped before readiness",
                scheduler.index()
            ));
        }
    }
    Ok(VmProtocolTaskServer::new(address, controls, threads))
}

fn bind_address(address: SocketAddr) -> io::Result<std_net::TcpListener> {
    let socket = Socket::new(
        Domain::for_address(address),
        Type::STREAM,
        Some(Protocol::TCP),
    )?;
    #[cfg(unix)]
    {
        socket.set_reuse_address(true)?;
        socket.set_reuse_port(true)?;
    }
    socket.set_nonblocking(true)?;
    socket.bind(&address.into())?;
    socket.listen(1_024)?;
    Ok(socket.into())
}

/// Bounded socket ingress and load accounting for one fixed VM owner.
struct VmProtocolShardIngress {
    scheduler: VmSchedulerId,
    sockets: VmLazyBoundedQueue<TcpStream>,
    load: VmProtocolShardLoad,
    owner_parked: VmProtocolOwnerParked,
    poll_waker: Arc<MioWaker>,
    capacity: Arc<VmProtocolCapacity>,
}

impl VmProtocolShardIngress {
    fn load(&self) -> usize {
        // Admission is a best-effort balancing heuristic; `reserve` remains
        // the authoritative capacity check. Relaxed samples avoid imposing a
        // cross-owner synchronization fence on every accepted connection.
        self.load.load(Ordering::Relaxed)
    }

    fn try_reserve(&self) -> bool {
        // The protocol acceptor is the only producer of reservations; owners
        // only decrement this counter on completion. One fetch-add therefore
        // enforces the exact bound without a compare-exchange retry loop.
        let prior = self.load.fetch_add(1, Ordering::Relaxed);
        if prior < MAX_TASKS_PER_SHARD {
            return true;
        }
        self.load.fetch_sub(1, Ordering::Relaxed);
        false
    }

    fn release_reservation(&self) {
        let prior = self.load.fetch_sub(1, Ordering::Relaxed);
        debug_assert!(prior > 0, "protocol shard reservation underflow");
    }

    fn admit_reserved(&self, stream: TcpStream, wake_owner: bool) -> Result<(), String> {
        if let Err(error) = self.sockets.push(stream) {
            self.release_reservation();
            drop(error.into_inner());
            return Err(format!(
                "error[vm.protocol_capacity]: scheduler {} ingress queue full",
                self.scheduler.index()
            ));
        }
        // Publish before sampling the parked flag. If the owner is still
        // active it observes the nonempty queue before polling; if it has
        // committed to the poll path, this wake breaks that sleep. Sampling
        // before publication permits the owner to enter `poll` in between and
        // strand a socket until unrelated readiness arrives.
        if wake_owner && self.owner_parked.load(Ordering::Acquire) {
            self.poll_waker.wake().map_err(|error| {
                format!(
                    "error[vm.protocol_scheduler]: wake scheduler {}: {error}",
                    self.scheduler.index()
                )
            })?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn admit(&self, stream: TcpStream, wake_owner: bool) -> Result<(), String> {
        if !self.try_reserve() {
            return Err(format!(
                "error[vm.protocol_capacity]: scheduler {} is full",
                self.scheduler.index()
            ));
        }
        self.admit_reserved(stream, wake_owner)
    }

    fn complete(&self) {
        let prior = self.load.fetch_sub(1, Ordering::Relaxed);
        debug_assert!(prior > 0, "protocol shard load underflow");
        self.capacity.notify();
    }
}

struct VmProtocolTaskShard {
    scheduler: VmSchedulerId,
    poll: Poll,
    owner_wake: Arc<VmProtocolOwnerWake>,
    events: Events,
    ready_events: Vec<VmReadyEvent>,
    factory: VmProtocolTaskFactory,
    tasks: Vec<Option<VmProtocolTask>>,
    task_wake_slots: Vec<VmProtocolTaskWakeSlot>,
    task_generations: Vec<usize>,
    free_task_slots: Vec<usize>,
    active_task_count: usize,
    scheduled: Arc<VmLazyBoundedQueue<Token>>,
    poll_waker: Arc<MioWaker>,
    control_port: Arc<VmProtocolControlPort>,
    ingress: Arc<VmProtocolShardIngress>,
    acceptor: Option<VmProtocolAcceptor>,
    capacity_notification_epoch: usize,
    process_ids: VmProtocolProcessIds,
}

/// Sendable bootstrap moved before any owner-local protocol future exists.
struct VmProtocolShardStartup {
    scheduler: VmSchedulerId,
    poll: Poll,
    owner_wake: Arc<VmProtocolOwnerWake>,
    events: Events,
    ready_events: Vec<VmReadyEvent>,
    factory: VmProtocolTaskFactory,
    scheduled: Arc<VmLazyBoundedQueue<Token>>,
    poll_waker: Arc<MioWaker>,
    control_port: Arc<VmProtocolControlPort>,
    ingress: Arc<VmProtocolShardIngress>,
    acceptor: Option<VmProtocolAcceptor>,
    capacity_notification_epoch: usize,
    process_ids: VmProtocolProcessIds,
}

impl VmProtocolShardStartup {
    fn new(
        scheduler: VmSchedulerId,
        factory: VmProtocolTaskFactory,
        capacity: Arc<VmProtocolCapacity>,
    ) -> Result<Self, String> {
        let poll = Poll::new().map_err(render_io("poller"))?;
        // One registry handle is shared by every socket on this owner. This
        // lets transports arm write readiness lazily without cloning an
        // epoll/kqueue descriptor for every connection.
        let scheduled = Arc::new(VmLazyBoundedQueue::bounded(MAX_TASKS_PER_SHARD));
        let poll_waker = Arc::new(
            MioWaker::new(poll.registry(), TASK_WAKE_TOKEN).map_err(render_io("task waker"))?,
        );
        let owner_wake = Arc::new(VmProtocolOwnerWake {
            scheduler,
            registry: poll
                .registry()
                .try_clone()
                .map_err(render_io("poll registry clone"))?,
            queue: Arc::clone(&scheduled),
            poll_waker: Arc::clone(&poll_waker),
        });
        // mio permits exactly one Waker per Poll. Capacity notifications
        // therefore share the owner waker and carry their reason through a
        // monotonic epoch rather than competing for another registry waker.
        capacity.register(&poll_waker)?;
        let capacity_notification_epoch = capacity.notification_epoch();
        let ingress = Arc::new(VmProtocolShardIngress {
            scheduler,
            sockets: VmLazyBoundedQueue::bounded(MAX_TASKS_PER_SHARD),
            load: VmProtocolShardLoad::new(0),
            owner_parked: VmProtocolOwnerParked::new(false),
            poll_waker: Arc::clone(&poll_waker),
            capacity,
        });
        let control_port = Arc::new(VmProtocolControlPort {
            scheduler,
            messages: VmLazyBoundedQueue::bounded(MAX_CONTROL_MESSAGES_PER_SHARD),
            poll_waker: Arc::clone(&poll_waker),
        });
        PROTOCOL_CONTROL_PORTS
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .map_err(|_| "error[vm.protocol_control]: registry lock poisoned".to_string())?
            .push(Arc::downgrade(&control_port));
        Ok(Self {
            scheduler,
            poll,
            owner_wake,
            events: Events::with_capacity(64),
            ready_events: Vec::with_capacity(64),
            factory,
            scheduled,
            poll_waker,
            control_port,
            ingress,
            acceptor: None,
            capacity_notification_epoch,
            process_ids: VmProtocolProcessIds::new()?,
        })
    }

    fn attach_acceptor(
        &mut self,
        listener: std_net::TcpListener,
        ingresses: Vec<Arc<VmProtocolShardIngress>>,
        local_index: usize,
        capacity: Arc<VmProtocolCapacity>,
    ) -> Result<(), String> {
        self.acceptor = Some(VmProtocolAcceptor::new(
            listener,
            self.poll.registry(),
            ingresses,
            local_index,
            capacity,
        )?);
        Ok(())
    }

    fn ingress(&self) -> Arc<VmProtocolShardIngress> {
        Arc::clone(&self.ingress)
    }

    /// Returns this scheduler's supervisor control port.
    fn control_port(&self) -> Arc<VmProtocolControlPort> {
        Arc::clone(&self.control_port)
    }

    /// Runs this scheduler-local protocol owner until shutdown or failure.
    fn run(self) -> Result<(), String> {
        VmProtocolTaskShard {
            scheduler: self.scheduler,
            poll: self.poll,
            owner_wake: self.owner_wake,
            events: self.events,
            ready_events: self.ready_events,
            factory: self.factory,
            tasks: Vec::new(),
            task_wake_slots: Vec::new(),
            task_generations: Vec::new(),
            free_task_slots: Vec::new(),
            active_task_count: 0,
            scheduled: self.scheduled,
            poll_waker: self.poll_waker,
            control_port: self.control_port,
            ingress: self.ingress,
            acceptor: self.acceptor,
            capacity_notification_epoch: self.capacity_notification_epoch,
            process_ids: self.process_ids,
        }
        .run()
    }
}

impl VmProtocolTaskShard {
    /// Services readiness, controls, and scheduled futures on one owner thread.
    fn run(mut self) -> Result<(), String> {
        let mut accept_pending = false;
        loop {
            wake_due_protocol_timers();
            let idle =
                self.ingress.sockets.is_empty() && !accept_pending && !has_owner_local_scheduled();
            if idle {
                self.ingress.owner_parked.store(true, Ordering::Release);
                // Close the observation-to-sleep race after publishing the
                // parked state. Producers either publish before this recheck
                // or observe `owner_parked` and wake the readiness poller.
                if self.ingress.sockets.is_empty()
                    && !accept_pending
                    && !has_owner_local_scheduled()
                {
                    self.poll
                        .poll(
                            &mut self.events,
                            next_protocol_timer_timeout(std::time::Instant::now()),
                        )
                        .map_err(render_io("readiness poll"))?;
                } else {
                    self.events.clear();
                }
                self.ingress.owner_parked.store(false, Ordering::Release);
            } else {
                self.events.clear();
            }
            self.drain_ingress();
            let mut ready_events = std::mem::take(&mut self.ready_events);
            ready_events.extend(self.events.iter().map(VmReadyEvent::from));
            let mut should_accept = accept_pending;
            accept_pending = false;
            for event in ready_events.iter().copied() {
                match event.token() {
                    ACCEPTOR_LISTENER_TOKEN => should_accept = true,
                    ACCEPTOR_CAPACITY_TOKEN if self.acceptor.is_some() => should_accept = true,
                    ACCEPTOR_CAPACITY_TOKEN => {}
                    TASK_WAKE_TOKEN => {}
                    token => self.publish_readiness(token, event),
                }
            }
            let capacity_notification_epoch = self.ingress.capacity.notification_epoch();
            if capacity_notification_epoch != self.capacity_notification_epoch {
                self.capacity_notification_epoch = capacity_notification_epoch;
                should_accept |= self.acceptor.is_some();
            }
            ready_events.clear();
            self.ready_events = ready_events;
            if should_accept {
                accept_pending = self
                    .acceptor
                    .as_mut()
                    .ok_or_else(|| {
                        "error[vm.protocol_accept]: readiness delivered to a non-acceptor shard"
                            .to_string()
                    })?
                    .accept_ready()?;
                self.drain_local_admissions();
                self.drain_ingress();
            }
            if self.drain_controls() {
                return Ok(());
            }
            wake_due_protocol_timers();
            self.poll_scheduled();
        }
    }

    fn drain_ingress(&mut self) {
        while let Ok(stream) = self.ingress.sockets.pop() {
            if let Err(error) = self.admit_stream(stream) {
                eprintln!("{error}");
                self.ingress.complete();
            }
        }
    }

    fn drain_local_admissions(&mut self) {
        let mut admissions = self
            .acceptor
            .as_mut()
            .expect("accept readiness requires an acceptor")
            .take_local_admissions();
        for stream in admissions.drain(..) {
            if let Err(error) = self.admit_stream(stream) {
                eprintln!("{error}");
                self.ingress.complete();
            }
        }
        self.acceptor
            .as_mut()
            .expect("accept readiness requires an acceptor")
            .recycle_local_admissions(admissions);
    }

    fn drain_controls(&self) -> bool {
        while let Ok(control) = self.control_port.messages.pop() {
            match control {
                VmProtocolControl::RetireResource(identity) => {
                    PROTOCOL_LOCAL_RESOURCES.with(|resources| {
                        if let Ok(mut resources) = resources.try_borrow_mut() {
                            resources.retire(identity);
                        }
                    });
                }
                VmProtocolControl::Shutdown => return true,
            }
        }
        false
    }

    fn admit_stream(&mut self, mut stream: TcpStream) -> Result<(), String> {
        if self.active_task_count >= MAX_TASKS_PER_SHARD {
            return Err(format!(
                "error[vm.protocol_capacity]: scheduler {} task table full",
                self.scheduler.index()
            ));
        }
        let (slot, token) = self.vacant_task_slot()?;
        if let Err(error) = self
            .poll
            .registry()
            .register(&mut stream, token, Interest::READABLE)
        {
            self.free_task_slots.push(slot);
            return Err(render_io("task registration")(error));
        }
        let route = self.process_ids.next_route(self.scheduler)?;
        self.prepare_task_wake_slot(slot, token);
        let future = (self.factory)(
            VmReadyTcpStream::new(stream, Arc::clone(&self.owner_wake), token),
            route,
        );
        self.tasks[slot] = Some(VmProtocolTask {
            token,
            route,
            future,
        });
        self.active_task_count += 1;
        // Registration precedes publication in the task table. Queue the
        // first poll owner-locally so accepted batches are fully admitted
        // before protocol work starts, while request bytes already delivered
        // with the connection avoid one guaranteed readiness-loop turn.
        // A client that has not written yet simply observes EAGAIN and parks
        // on the registration already installed above.
        self.task_wake_slots[slot].waker.wake_by_ref();
        Ok(())
    }

    fn publish_readiness(&mut self, token: Token, event: VmReadyEvent) {
        let Some(task) = self.task_for_token(token) else {
            return;
        };
        let wake = VmSocketReadinessWake {
            route: task.route,
            readable: event.readable,
            writable: event.writable,
            closed: event.closed,
        };
        debug_assert_eq!(wake.route, task.route);
        if (wake.readable || wake.writable || wake.closed)
            && !self.task_wake_slots[task_slot_for_token(token).expect("validated task token")]
                .wake
                .scheduled
                .load(Ordering::Acquire)
        {
            self.poll_task(token);
        }
    }

    fn poll_scheduled(&mut self) {
        for _ in 0..MAX_TASK_POLLS_PER_TICK {
            let token = match pop_owner_local_scheduled() {
                Some(token) => token,
                None => {
                    let Ok(token) = self.scheduled.pop() else {
                        return;
                    };
                    token
                }
            };
            self.poll_task(token);
        }
        if !self.scheduled.is_empty() {
            let _ = self.poll_waker.wake();
        }
    }

    fn poll_task(&mut self, token: Token) {
        let outcome = {
            let Some(slot) = task_slot_for_token(token) else {
                return;
            };
            let Some(task) = self.tasks.get_mut(slot).and_then(Option::as_mut) else {
                return;
            };
            if task.token != token {
                return;
            }
            let wake_slot = &self.task_wake_slots[slot];
            wake_slot.wake.scheduled.store(false, Ordering::Release);
            let mut context = Context::from_waker(&wake_slot.waker);
            with_protocol_task(task.route, || task.future.as_mut().poll(&mut context))
        };
        match outcome {
            TaskPoll::Pending => {}
            TaskPoll::Ready(Ok(())) => {
                self.remove_task(token);
                self.ingress.complete();
            }
            TaskPoll::Ready(Err(error)) => {
                eprintln!("error[vm.protocol_task]: {error}");
                self.remove_task(token);
                self.ingress.complete();
            }
        }
    }

    fn task_for_token(&self, token: Token) -> Option<&VmProtocolTask> {
        let slot = task_slot_for_token(token)?;
        self.tasks
            .get(slot)
            .and_then(Option::as_ref)
            .filter(|task| task.token == token)
    }

    fn remove_task(&mut self, token: Token) {
        let Some(slot) = task_slot_for_token(token) else {
            return;
        };
        let Some(task) = self.tasks.get_mut(slot) else {
            return;
        };
        if task.as_ref().is_none_or(|task| task.token != token) {
            return;
        }
        *task = None;
        self.free_task_slots.push(slot);
        self.active_task_count -= 1;
    }

    fn prepare_task_wake_slot(&mut self, slot: usize, token: Token) {
        if slot == self.task_wake_slots.len() {
            self.task_wake_slots.push(VmProtocolTaskWakeSlot::new(
                token,
                Arc::clone(&self.owner_wake),
            ));
            return;
        }
        let wake_slot = &mut self.task_wake_slots[slot];
        if Arc::strong_count(&wake_slot.wake) == 2 {
            wake_slot.wake.token.store(token.0, Ordering::Release);
            wake_slot.wake.scheduled.store(false, Ordering::Release);
        } else {
            // A completed future may have handed a waker clone to an
            // asynchronous operation that outlives it. Keep that generation
            // immutable and install a fresh slot so its eventual wake is
            // rejected by the generation-qualified task token.
            *wake_slot = VmProtocolTaskWakeSlot::new(token, Arc::clone(&self.owner_wake));
        }
    }

    fn vacant_task_slot(&mut self) -> Result<(usize, Token), String> {
        let slot = if let Some(slot) = self.free_task_slots.pop() {
            slot
        } else {
            let slot = self.tasks.len();
            if slot >= MAX_TASKS_PER_SHARD {
                return Err("error[vm.protocol_capacity]: task table full".to_string());
            }
            self.tasks.push(None);
            self.task_generations.push(0);
            slot
        };
        let generation = self.task_generations[slot]
            .checked_add(1)
            .ok_or_else(|| "error[vm.protocol_token]: task generation exhausted".to_string())?;
        self.task_generations[slot] = generation;
        let token = generation
            .checked_mul(MAX_TASKS_PER_SHARD)
            .and_then(|base| base.checked_add(slot))
            .and_then(|value| value.checked_add(FIRST_TASK_TOKEN))
            .map(Token)
            .ok_or_else(|| "error[vm.protocol_token]: task token space exhausted".to_string())?;
        Ok((slot, token))
    }
}

fn task_slot_for_token(token: Token) -> Option<usize> {
    let encoded = token.0.checked_sub(FIRST_TASK_TOKEN)?;
    let slot = encoded % MAX_TASKS_PER_SHARD;
    if slot < MAX_TASKS_PER_SHARD {
        Some(slot)
    } else {
        None
    }
}

/// Supervisor command consumed only by one fixed protocol owner.
enum VmProtocolControl {
    /// Retire one scheduler-local typed resource generation.
    RetireResource(u64),
    /// Stop admission and drop all connection futures on this owner.
    Shutdown,
}

struct VmProtocolControlPort {
    scheduler: VmSchedulerId,
    messages: VmLazyBoundedQueue<VmProtocolControl>,
    poll_waker: Arc<MioWaker>,
}

struct VmProtocolTask {
    token: Token,
    route: VmProtocolTaskRoute,
    future: VmProtocolTaskFuture,
}

/// Allocates one stable connection-task route on a fixed protocol scheduler.
#[cfg(test)]
pub(crate) fn next_protocol_task_route(
    scheduler: VmSchedulerId,
) -> Result<VmProtocolTaskRoute, String> {
    VmProtocolProcessIds::new()?.next_route(scheduler)
}

#[cfg(test)]
#[path = "protocol_task_executor_test.rs"]
#[cfg(test)]
mod protocol_task_executor_test;
