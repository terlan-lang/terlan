//! Descriptor flag preservation and readiness semantics for child pipe owners.

use super::*;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

#[test]
fn nonblocking_preserves_existing_flags_and_is_idempotent() {
    use rustix::fs::{fcntl_getfl, OFlags};
    let (reader, _writer) = UnixStream::pair().unwrap();
    let before = fcntl_getfl(&reader).unwrap();
    set_nonblocking(&reader).unwrap();
    set_nonblocking(&reader).unwrap();
    assert_eq!(fcntl_getfl(&reader).unwrap(), before | OFlags::NONBLOCK);
}

#[test]
fn readiness_keeps_empty_and_closed_streams_distinct() {
    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    set_nonblocking(&reader).unwrap();
    let mut byte = [0];
    wait(&reader, false, Duration::ZERO).unwrap();
    assert_eq!(
        reader.read(&mut byte).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    wait(&writer, true, Duration::ZERO).unwrap();
    writer.write_all(b"x").unwrap();
    wait(&reader, false, Duration::from_millis(10)).unwrap();
    assert_eq!(reader.read(&mut byte).unwrap(), 1);
    assert_eq!(byte, *b"x");
    drop(writer);
    wait(&reader, false, Duration::from_millis(10)).unwrap();
    assert_eq!(reader.read(&mut byte).unwrap(), 0);
}
