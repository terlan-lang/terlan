//! Deadline-owned pipes, independent of whether every descendant stays in a group.

use super::NativeBoundaryCancellationToken;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct State {
    started: Instant,
    timeout: Duration,
    cancellation: Option<NativeBoundaryCancellationToken>,
    stopped: AtomicBool,
}

impl State {
    fn failure(&self) -> Option<(&'static str, String)> {
        if self
            .cancellation
            .as_ref()
            .is_some_and(NativeBoundaryCancellationToken::is_cancelled)
        {
            Some(("cancelled", "child process was cancelled".to_string()))
        } else if self.started.elapsed() >= self.timeout {
            Some((
                "timed_out",
                "child pipes exceeded the request wall-clock timeout".to_string(),
            ))
        } else {
            None
        }
    }

    fn check(&self) -> io::Result<()> {
        if let Some((code, message)) = self.failure() {
            let kind = if code == "timed_out" {
                io::ErrorKind::TimedOut
            } else {
                io::ErrorKind::Other
            };
            return Err(io::Error::new(kind, message));
        }
        if self.stopped.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "child pipe owner stopped",
            ));
        }
        Ok(())
    }
}

pub(super) struct PipeScope(Arc<State>);

impl PipeScope {
    pub(super) fn new(
        started: Instant,
        timeout: Duration,
        cancellation: Option<&NativeBoundaryCancellationToken>,
    ) -> Self {
        Self(Arc::new(State {
            started,
            timeout,
            cancellation: cancellation.cloned(),
            stopped: AtomicBool::new(false),
        }))
    }

    pub(super) fn failure(&self) -> Option<(&'static str, String)> {
        self.0.failure()
    }

    pub(super) fn stop(&self) {
        self.0.stopped.store(true, Ordering::Release);
    }

    /// Keep the deadline owner active until every pipe worker has finished.
    pub(super) fn drain(
        &self,
        overflow: &AtomicBool,
        complete: impl Fn() -> bool,
    ) -> Option<(&'static str, String)> {
        loop {
            let failure = self.failure().or_else(|| {
                overflow.load(Ordering::Acquire).then(|| {
                    (
                        "output_limit_exceeded",
                        "child output exceeded its byte limit".to_string(),
                    )
                })
            });
            if failure.is_some() {
                self.stop();
                return failure;
            }
            if complete() {
                return None;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(super) fn wrap<T: std::os::fd::AsFd>(&self, pipe: T) -> io::Result<ScopedPipe<T>> {
        terlan_process_owner::pipe::set_nonblocking(&pipe)?;
        Ok(ScopedPipe {
            pipe,
            state: Arc::clone(&self.0),
            wait: |pipe, writing, timeout| terlan_process_owner::pipe::wait(pipe, writing, timeout),
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(super) fn wrap<T>(&self, pipe: T) -> io::Result<ScopedPipe<T>> {
        Ok(ScopedPipe {
            pipe,
            state: Arc::clone(&self.0),
            wait: |_, _, timeout| {
                std::thread::sleep(timeout);
                Ok(())
            },
        })
    }
}

impl Drop for PipeScope {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(super) struct ScopedPipe<T> {
    pipe: T,
    state: Arc<State>,
    wait: fn(&T, bool, Duration) -> io::Result<()>,
}

impl<T> ScopedPipe<T> {
    fn operate<R>(
        &mut self,
        writing: bool,
        mut operation: impl FnMut(&mut T) -> io::Result<R>,
    ) -> io::Result<R> {
        loop {
            self.state.check()?;
            match operation(&mut self.pipe) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    // Readiness wakes immediately for I/O; the small maximum
                    // wait bounds cancellation even when the peer stays idle.
                    let timeout = self
                        .state
                        .timeout
                        .saturating_sub(self.state.started.elapsed())
                        .min(Duration::from_millis(10));
                    (self.wait)(&self.pipe, writing, timeout)?;
                }
                result => return result,
            }
        }
    }
}

impl<T: Read> Read for ScopedPipe<T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.operate(false, |pipe| pipe.read(bytes))
    }
}

impl<T: Write> Write for ScopedPipe<T> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.operate(true, |pipe| pipe.write(bytes))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.operate(true, Write::flush)
    }
}
