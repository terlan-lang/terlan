use std::collections::VecDeque;
use std::io;

pub(super) struct FailingReader {
    message: &'static str,
}

impl FailingReader {
    pub(super) fn new(message: &'static str) -> Self {
        Self { message }
    }
}

impl io::Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other(self.message))
    }
}

pub(super) struct ChunkedReader {
    chunks: VecDeque<Vec<u8>>,
}

impl ChunkedReader {
    pub(super) fn new(chunks: Vec<Vec<u8>>) -> Self {
        Self {
            chunks: chunks.into(),
        }
    }
}

impl io::Read for ChunkedReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let Some(chunk) = self.chunks.pop_front() else {
            return Ok(0);
        };
        let length = chunk.len().min(buffer.len());
        buffer[..length].copy_from_slice(&chunk[..length]);
        if length < chunk.len() {
            self.chunks.push_front(chunk[length..].to_vec());
        }
        Ok(length)
    }
}

pub(super) struct FailingWriter {
    remaining_calls: usize,
    message: &'static str,
}

impl FailingWriter {
    pub(super) fn new(write_calls_before_failure: usize, message: &'static str) -> Self {
        Self {
            remaining_calls: write_calls_before_failure,
            message,
        }
    }
}

impl io::Write for FailingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.remaining_calls == 0 {
            return Err(io::Error::other(self.message));
        }
        self.remaining_calls -= 1;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) struct BodyFailingWriter {
    wrote_head: bool,
    message: &'static str,
}

impl BodyFailingWriter {
    pub(super) fn new(message: &'static str) -> Self {
        Self {
            wrote_head: false,
            message,
        }
    }
}

impl io::Write for BodyFailingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.wrote_head {
            return Err(io::Error::other(self.message));
        }
        self.wrote_head = true;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
