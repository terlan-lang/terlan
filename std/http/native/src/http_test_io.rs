//! Shared fragmented, fault-injectable transport for maintained HTTP codec tests.

use hyper::rt::ReadBufCursor;
use std::collections::VecDeque;
use std::future::Future;
use std::io::{self, Read, Write};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

#[derive(Default)]
pub(crate) struct State {
    pub(crate) incoming: VecDeque<u8>,
    pub(crate) outgoing: Vec<u8>,
    pub(crate) reads: usize,
    pub(crate) read_limit: Option<usize>,
    pub(crate) drops: usize,
    pub(crate) eof: bool,
    pub(crate) write_error: Option<io::ErrorKind>,
    pub(crate) write_budget: Option<usize>,
    pub(crate) flush_error: Option<io::ErrorKind>,
}

pub(crate) struct MemoryIo<const KIND: u8>(pub(crate) Arc<Mutex<State>>);

impl<const KIND: u8> Drop for MemoryIo<KIND> {
    fn drop(&mut self) {
        self.0.lock().unwrap().drops += 1;
    }
}

impl<const KIND: u8> Read for MemoryIo<KIND> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut state = self.0.lock().unwrap();
        state.reads += 1;
        if !buffer.is_empty() && state.incoming.is_empty() && !state.eof {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = buffer
            .len()
            .min(state.incoming.len())
            .min(state.read_limit.unwrap_or(usize::MAX));
        for byte in &mut buffer[..count] {
            *byte = state.incoming.pop_front().unwrap();
        }
        Ok(count)
    }
}

impl<const KIND: u8> Write for MemoryIo<KIND> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let mut state = self.0.lock().unwrap();
        if let Some(error) = state.write_error {
            return Err(error.into());
        }
        if state.write_budget == Some(0) {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = buffer
            .len()
            .min(7)
            .min(state.write_budget.unwrap_or(usize::MAX));
        state.outgoing.extend_from_slice(&buffer[..count]);
        if let Some(budget) = &mut state.write_budget {
            *budget -= count;
        }
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.0.lock().unwrap().flush_error {
            Some(error) => Err(error.into()),
            None => Ok(()),
        }
    }
}

impl<const KIND: u8> hyper::rt::Read for MemoryIo<KIND> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        mut buffer: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let mut bytes = [0; 4096];
        let capacity = buffer.remaining().min(bytes.len());
        match Read::read(&mut *self, &mut bytes[..capacity]) {
            Ok(count) => {
                buffer.put_slice(&bytes[..count]);
                Poll::Ready(Ok(()))
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Poll::Pending,
            Err(error) => Poll::Ready(Err(error)),
        }
    }
}

impl<const KIND: u8> hyper::rt::Write for MemoryIo<KIND> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Write::write(&mut *self, bytes))
    }

    fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Write::flush(&mut *self))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

pub(crate) fn complete<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..1000 {
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return result;
        }
    }
    panic!("in-memory HTTP operation did not complete");
}
