use std::{collections::BTreeSet, sync::Arc, time::Duration};

use polling::{Event, Events, Poller};

use super::{CAbiError, Connection};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionInterest {
    Drive,
    Read,
    Write,
}

#[derive(Clone, Copy)]
pub struct ConnectionReadiness<'a> {
    pub key: u64,
    pub connection: &'a Connection,
    pub interest: ConnectionInterest,
}

#[derive(Clone, Debug)]
pub struct ConnectionPoller {
    poller: Arc<Poller>,
}

impl ConnectionPoller {
    pub fn new() -> Result<Self, ReadinessError> {
        Poller::new()
            .map(|poller| Self {
                poller: Arc::new(poller),
            })
            .map_err(|error| ReadinessError::io("postgres.readiness.create", error))
    }

    pub fn notify(&self) -> Result<(), ReadinessError> {
        self.poller
            .notify()
            .map_err(|error| ReadinessError::io("postgres.readiness.notify", error))
    }

    pub fn wait(
        &self,
        sources: &[ConnectionReadiness<'_>],
        timeout: Option<Duration>,
    ) -> Result<Vec<u64>, ReadinessError> {
        if let Some(source) = sources
            .iter()
            .find(|source| source.interest == ConnectionInterest::Drive)
        {
            return Ok(vec![source.key]);
        }
        let sockets = socket_sources(sources)?;
        validate_unique_sockets(&sockets)?;

        let mut registered = Vec::with_capacity(sockets.len());
        for (index, source) in sockets.iter().enumerate() {
            let event = match source.interest {
                ConnectionInterest::Read => Event::readable(index),
                ConnectionInterest::Write => Event::writable(index),
                ConnectionInterest::Drive => unreachable!("drive sources return early"),
            };
            // SAFETY: each descriptor is borrowed through `ConnectionReadiness` for
            // this call and every registration is deleted before those borrows end.
            if let Err(error) = unsafe { self.poller.add(source.socket, event) } {
                let _ = cleanup(&self.poller, &registered);
                return Err(ReadinessError::io("postgres.readiness.register", error));
            }
            registered.push(source.socket);
        }

        let mut events = Events::new();
        let waited = self
            .poller
            .wait(&mut events, timeout)
            .map_err(|error| ReadinessError::io("postgres.readiness.wait", error));
        let cleanup_result = cleanup(&self.poller, &registered);
        waited?;
        cleanup_result?;

        Ok(events
            .iter()
            .filter_map(|event| sources.get(event.key).map(|source| source.key))
            .collect())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadinessError {
    pub operation: &'static str,
    pub message: String,
}

impl ReadinessError {
    fn io(operation: &'static str, error: std::io::Error) -> Self {
        Self {
            operation,
            message: error.to_string(),
        }
    }
}

struct SocketSource {
    socket: platform::RawSource,
    interest: ConnectionInterest,
}

fn socket_sources(
    sources: &[ConnectionReadiness<'_>],
) -> Result<Vec<SocketSource>, ReadinessError> {
    sources
        .iter()
        .map(|source| {
            let socket = source.connection.socket().map_err(readiness_c_abi_error)?;
            Ok(SocketSource {
                socket: platform::raw_source(socket)?,
                interest: source.interest,
            })
        })
        .collect()
}

fn validate_unique_sockets(sources: &[SocketSource]) -> Result<(), ReadinessError> {
    let mut sockets = BTreeSet::new();
    for source in sources {
        if !sockets.insert(source.socket) {
            return Err(ReadinessError {
                operation: "postgres.readiness.duplicate_socket",
                message: "one libpq socket has multiple readiness owners".to_string(),
            });
        }
    }
    Ok(())
}

fn cleanup(poller: &Poller, registered: &[platform::RawSource]) -> Result<(), ReadinessError> {
    let mut first_error = None;
    for raw in registered {
        if let Err(error) = platform::delete(poller, *raw) {
            first_error
                .get_or_insert_with(|| ReadinessError::io("postgres.readiness.unregister", error));
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn readiness_c_abi_error(error: CAbiError) -> ReadinessError {
    ReadinessError {
        operation: error.operation,
        message: format!("libpq status {}", error.status),
    }
}

#[cfg(unix)]
mod platform {
    use std::os::fd::{BorrowedFd, RawFd};

    use polling::Poller;

    use super::ReadinessError;

    pub(super) type RawSource = RawFd;

    pub(super) fn raw_source(socket: i64) -> Result<RawSource, ReadinessError> {
        RawFd::try_from(socket).map_err(|_| ReadinessError {
            operation: "postgres.readiness.invalid_socket",
            message: format!("invalid Unix socket {socket}"),
        })
    }

    pub(super) fn delete(poller: &Poller, raw: RawSource) -> std::io::Result<()> {
        // SAFETY: the owning connection is borrowed for the entire wait call.
        poller.delete(unsafe { BorrowedFd::borrow_raw(raw) })
    }
}

#[cfg(windows)]
mod platform {
    use std::os::windows::io::{BorrowedSocket, RawSocket};

    use polling::Poller;

    use super::ReadinessError;

    pub(super) type RawSource = RawSocket;

    pub(super) fn raw_source(socket: i64) -> Result<RawSource, ReadinessError> {
        RawSocket::try_from(socket).map_err(|_| ReadinessError {
            operation: "postgres.readiness.invalid_socket",
            message: format!("invalid Windows socket {socket}"),
        })
    }

    pub(super) fn delete(poller: &Poller, raw: RawSource) -> std::io::Result<()> {
        // SAFETY: the owning connection is borrowed for the entire wait call.
        poller.delete(unsafe { BorrowedSocket::borrow_raw(raw) })
    }
}

#[cfg(not(any(unix, windows)))]
compile_error!("libpq readiness requires a Unix or Windows host");

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, thread, time::Duration};

    use super::ConnectionPoller;

    #[test]
    fn notification_wakes_an_unbounded_empty_wait() {
        let poller = ConnectionPoller::new().expect("create poller");
        let waiter = poller.clone();
        let (done, completed) = mpsc::channel();
        let worker = thread::spawn(move || {
            done.send(waiter.wait(&[], None)).expect("publish wait");
        });

        thread::sleep(Duration::from_millis(10));
        poller.notify().expect("notify poller");

        assert_eq!(
            completed
                .recv_timeout(Duration::from_secs(1))
                .expect("wait must wake")
                .expect("wait succeeds"),
            Vec::<u64>::new()
        );
        worker.join().expect("join waiter");
    }
}
