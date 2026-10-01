use super::*;
use mio::{event::Source, Registry};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

#[derive(Default)]
struct Observations {
    registered: AtomicUsize,
    armed: AtomicUsize,
    closed: AtomicUsize,
    dropped: AtomicUsize,
    output: Mutex<Vec<u8>>,
}

struct MemoryStream {
    state: Arc<Observations>,
    fail_registration: bool,
    blocked_write: bool,
}

impl MemoryStream {
    fn boxed(state: &Arc<Observations>, fail_registration: bool) -> Box<dyn ReadinessStream> {
        Box::new(Self {
            state: Arc::clone(state),
            fail_registration,
            blocked_write: true,
        })
    }
}

impl Drop for MemoryStream {
    fn drop(&mut self) {
        self.state.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

impl Source for MemoryStream {
    fn register(&mut self, _: &Registry, token: Token, interests: Interest) -> io::Result<()> {
        assert!(token.0 >= FIRST_TASK_TOKEN);
        assert_eq!(interests, Interest::READABLE);
        self.state.registered.fetch_add(1, Ordering::SeqCst);
        if self.fail_registration {
            Err(io::ErrorKind::PermissionDenied.into())
        } else {
            Ok(())
        }
    }
    fn reregister(&mut self, _: &Registry, _: Token, interests: Interest) -> io::Result<()> {
        assert_eq!(interests, Interest::READABLE.add(Interest::WRITABLE));
        self.state.armed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn deregister(&mut self, _: &Registry) -> io::Result<()> {
        Ok(())
    }
}

impl Read for MemoryStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        b"package input".as_slice().read(buffer)
    }
}
impl Write for MemoryStream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if std::mem::take(&mut self.blocked_write) {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        self.state.output.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl ReadinessStream for MemoryStream {
    fn shutdown_write(&self) -> io::Result<()> {
        self.state.closed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct Incoming {
    streams: VecDeque<Box<dyn ReadinessStream>>,
    error: io::ErrorKind,
    attempts: Arc<AtomicUsize>,
}
impl Source for Incoming {
    fn register(&mut self, _: &Registry, token: Token, interests: Interest) -> io::Result<()> {
        assert_eq!(token, ACCEPTOR_LISTENER_TOKEN);
        assert_eq!(interests, Interest::READABLE);
        Ok(())
    }
    fn reregister(&mut self, _: &Registry, _: Token, _: Interest) -> io::Result<()> {
        Ok(())
    }
    fn deregister(&mut self, _: &Registry) -> io::Result<()> {
        Ok(())
    }
}
impl IncomingStreams for Incoming {
    fn accept(&mut self) -> io::Result<Box<dyn ReadinessStream>> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        self.streams.pop_front().ok_or_else(|| self.error.into())
    }
}

fn startup(factory: VmProtocolTaskFactory) -> VmProtocolShardStartup {
    VmProtocolShardStartup::new(VmSchedulerId::primary(), factory, Arc::default()).unwrap()
}

fn run_shard(shard: VmProtocolShardStartup) -> VmProtocolTaskServer {
    let control = shard.control_port();
    let scheduler = shard.scheduler;
    let thread = thread::spawn(move || {
        CURRENT_PROTOCOL_SCHEDULER.with(|owner| owner.set(Some(scheduler)));
        shard.run()
    });
    VmProtocolTaskServer::new(vec![control], vec![thread])
}

fn make_acceptor(
    shard: &VmProtocolShardStartup,
    streams: VecDeque<Box<dyn ReadinessStream>>,
    error: io::ErrorKind,
) -> (VmProtocolAcceptor, Arc<AtomicUsize>) {
    let attempts = Arc::new(AtomicUsize::new(0));
    let acceptor = VmProtocolAcceptor::new(
        Box::new(Incoming {
            streams,
            error,
            attempts: Arc::clone(&attempts),
        }),
        shard.poll.registry(),
        vec![shard.ingress()],
        0,
        Arc::clone(&shard.ingress.capacity),
    )
    .unwrap();
    (acceptor, attempts)
}

#[test]
fn package_stream_runs_on_vm_owner_and_releases_registration_failures() {
    let (tx, rx) = mpsc::channel();
    let factory: VmProtocolTaskFactory = Arc::new(move |mut stream, route| {
        let tx = tx.clone();
        Box::pin(async move {
            assert_eq!(current_protocol_scheduler(), Some(route.scheduler()));
            assert_eq!(current_protocol_task_route(), Some(route));
            let mut bytes = [0; 13];
            assert_eq!(stream.read(&mut bytes).unwrap(), 13);
            assert_eq!(&bytes, b"package input");
            std::future::poll_fn(|context| match stream.write(b"reply") {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    context.waker().wake_by_ref();
                    TaskPoll::Pending
                }
                result => TaskPoll::Ready(result),
            })
            .await
            .unwrap();
            stream.shutdown_write().unwrap();
            tx.send(route).unwrap();
            Ok(())
        })
    });
    let shard = startup(factory);
    let state = Arc::new(Observations::default());
    let failed = Arc::new(Observations::default());
    let (mut acceptor, _) = make_acceptor(
        &shard,
        [
            MemoryStream::boxed(&failed, true),
            MemoryStream::boxed(&state, false),
        ]
        .into(),
        io::ErrorKind::WouldBlock,
    );
    assert!(!acceptor.accept_ready().unwrap());
    for stream in acceptor.take_local_admissions() {
        assert!(shard.ingress.admit_reserved(stream, false).is_ok());
    }
    let ingress = shard.ingress();
    let scheduler = shard.scheduler;
    let mut server = run_shard(shard);
    let completed = rx.recv_timeout(Duration::from_secs(3));
    server.stop().unwrap();
    assert_eq!(completed.unwrap().scheduler(), scheduler);
    assert_eq!(state.registered.load(Ordering::SeqCst), 1);
    assert_eq!(state.armed.load(Ordering::SeqCst), 1);
    assert_eq!(state.closed.load(Ordering::SeqCst), 1);
    assert_eq!(state.dropped.load(Ordering::SeqCst), 1);
    assert_eq!(*state.output.lock().unwrap(), b"reply");
    assert_eq!(failed.dropped.load(Ordering::SeqCst), 1);
    assert_eq!(failed.closed.load(Ordering::SeqCst), 0);
    assert_eq!(ingress.load.load(Ordering::SeqCst), 0);
}

#[test]
fn package_acceptance_is_bounded_and_interrupted_calls_cannot_spin() {
    let shard = startup(Arc::new(|_, _| panic!("must not schedule")));
    let state = Arc::new(Observations::default());
    let streams = (0..MAX_ACCEPTS_PER_TICK + 1)
        .map(|_| MemoryStream::boxed(&state, false))
        .collect();
    let (mut acceptor, attempts) = make_acceptor(&shard, streams, io::ErrorKind::WouldBlock);
    assert!(acceptor.accept_ready().unwrap());
    assert_eq!(attempts.load(Ordering::SeqCst), MAX_ACCEPTS_PER_TICK);
    assert_eq!(acceptor.take_local_admissions().len(), MAX_ACCEPTS_PER_TICK);
    assert!(!acceptor.accept_ready().unwrap());
    drop(acceptor);
    assert_eq!(
        state.dropped.load(Ordering::SeqCst),
        MAX_ACCEPTS_PER_TICK + 1
    );

    let (mut interrupted, attempts) =
        make_acceptor(&shard, VecDeque::new(), io::ErrorKind::Interrupted);
    assert!(interrupted.accept_ready().unwrap());
    assert_eq!(attempts.load(Ordering::SeqCst), MAX_ACCEPTS_PER_TICK);
    let (mut broken, _) = make_acceptor(&shard, VecDeque::new(), io::ErrorKind::ConnectionAborted);
    assert!(broken
        .accept_ready()
        .unwrap_err()
        .contains("error[vm.protocol_accept]"));
}

#[test]
fn package_acceptance_parks_one_stream_until_capacity_returns() {
    let shard = startup(Arc::new(|_, _| panic!("must not schedule")));
    for _ in 0..MAX_TASKS_PER_SHARD {
        assert!(shard.ingress.try_reserve());
    }
    let state = Arc::new(Observations::default());
    let (mut acceptor, attempts) = make_acceptor(
        &shard,
        [MemoryStream::boxed(&state, false)].into(),
        io::ErrorKind::WouldBlock,
    );
    assert!(!acceptor.accept_ready().unwrap());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert!(acceptor.take_local_admissions().is_empty());
    assert!(shard.ingress.capacity.waiting.load(Ordering::Acquire));
    shard.ingress.release_reservation();
    assert!(!acceptor.accept_ready().unwrap());
    assert_eq!(acceptor.take_local_admissions().len(), 1);
    assert_eq!(state.dropped.load(Ordering::SeqCst), 1);
}

#[test]
fn stopping_owner_cancels_pending_package_stream_without_an_io_completion() {
    let (tx, rx) = mpsc::channel();
    let factory: VmProtocolTaskFactory = Arc::new(move |stream, _route| {
        let mut tx = Some(tx.clone());
        Box::pin(std::future::poll_fn(move |_| {
            let _keep_stream_owned = &stream;
            if let Some(tx) = tx.take() {
                tx.send(()).unwrap();
            }
            TaskPoll::Pending
        }))
    });
    let shard = startup(factory);
    let state = Arc::new(Observations::default());
    shard
        .ingress
        .admit(MemoryStream::boxed(&state, false), false)
        .unwrap();
    let mut server = run_shard(shard);
    let started = rx.recv_timeout(Duration::from_secs(3));
    server.stop().unwrap();
    started.unwrap();
    assert_eq!(state.dropped.load(Ordering::SeqCst), 1);
    assert_eq!(state.closed.load(Ordering::SeqCst), 0);
    assert!(state.output.lock().unwrap().is_empty());
}
