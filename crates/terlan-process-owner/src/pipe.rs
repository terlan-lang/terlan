//! Shared nonblocking descriptor operations for deadline-owned child pipes.
//!
//! Callers retain their cancellation and deadline policy. Readiness only permits
//! another I/O attempt; it does not promise bytes or extend the caller's deadline.

use std::io;
use std::os::fd::AsFd;
use std::time::Duration;

/// Enables nonblocking I/O without replacing the descriptor's other flags.
pub fn set_nonblocking(pipe: impl AsFd) -> io::Result<()> {
    use rustix::fs::{fcntl_getfl, fcntl_setfl, OFlags};
    fcntl_setfl(&pipe, fcntl_getfl(&pipe)? | OFlags::NONBLOCK)?;
    Ok(())
}

/// Waits at most `timeout` for the requested direction or an interrupt.
///
/// Timeout and interruption return control to the caller to check its deadline
/// and cancellation state. Hangup/error readiness is resolved by the next I/O.
pub fn wait(pipe: impl AsFd, writing: bool, timeout: Duration) -> io::Result<()> {
    use rustix::event::{poll, PollFd, PollFlags, Timespec};
    let flags = if writing {
        PollFlags::OUT
    } else {
        PollFlags::IN
    };
    let timeout = Timespec::try_from(timeout).map_err(io::Error::other)?;
    match poll(&mut [PollFd::new(&pipe, flags)], Some(&timeout)) {
        Ok(_) | Err(rustix::io::Errno::INTR) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
#[path = "pipe_test.rs"]
mod tests;
