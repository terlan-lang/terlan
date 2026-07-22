use super::super::tcp::VmTcpRuntime;
use super::{VmFramingError, VmInMemoryFrameReader};

fn connected_readers(limit: usize) -> (VmTcpRuntime, VmInMemoryFrameReader, VmInMemoryFrameReader) {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("fixture").expect("listen");
    let client = tcp.connect("fixture", "client").expect("connect");
    let server = tcp
        .accept(listener, "server")
        .expect("accept")
        .expect("server stream");
    (
        tcp,
        VmInMemoryFrameReader::new(client, limit).expect("client reader"),
        VmInMemoryFrameReader::new(server, limit).expect("server reader"),
    )
}

/// Verifies raw read/write and close operations through the framing fixture.
///
/// Inputs: one connected in-memory VM stream pair.
/// Output: test passes when raw bytes move and close state is visible.
/// Transformation: locks the common stream API before protocol-specific
/// framing uses it.
#[test]
fn vm_framing_fixture_reads_writes_and_closes_raw_streams() {
    let (mut tcp, mut client, mut server) = connected_readers(64);

    assert_eq!(client.write(&mut tcp, b"hello".to_vec()).expect("write"), 5);
    assert_eq!(
        server.read(&mut tcp, 64).expect("read"),
        Some(b"hello".to_vec())
    );
    assert_eq!(server.read(&mut tcp, 64).expect("empty"), None);

    client.close(&mut tcp).expect("close");
    assert!(tcp.inspect_stream(client.stream()).expect("inspect").closed);
}

/// Verifies exact reads preserve partial frame state across polls.
///
/// Inputs: one stream where the sender writes a frame in two chunks.
/// Output: test passes when the first poll is pending and the second poll
/// returns the full exact frame.
/// Transformation: proves scheduler polls do not lose already received bytes.
#[test]
fn vm_framing_fixture_preserves_partial_exact_frame_across_polls() {
    let (mut tcp, mut client, mut server) = connected_readers(64);

    client
        .write(&mut tcp, b"ab".to_vec())
        .expect("write partial");
    assert_eq!(server.read_exact(&mut tcp, 5).expect("pending"), None);
    assert_eq!(server.buffered_len(), 2);

    client.write(&mut tcp, b"cde".to_vec()).expect("write rest");
    assert_eq!(
        server.read_exact(&mut tcp, 5).expect("complete"),
        Some(b"abcde".to_vec())
    );
    assert_eq!(server.buffered_len(), 0);
}

/// Verifies raw reads drain staged frame bytes before touching TCP again.
///
/// Inputs: an incomplete exact read that leaves bytes in the frame buffer.
/// Output: test passes when a later raw read returns the staged bytes.
/// Transformation: keeps mixed raw/framed consumption deterministic.
#[test]
fn vm_framing_fixture_raw_read_drains_staged_bytes_first() {
    let (mut tcp, mut client, mut server) = connected_readers(64);

    client
        .write(&mut tcp, b"abc".to_vec())
        .expect("write partial");
    assert_eq!(server.read_exact(&mut tcp, 5).expect("pending"), None);

    assert_eq!(
        server.read(&mut tcp, 2).expect("staged read"),
        Some(b"ab".to_vec())
    );
    assert_eq!(
        server.read(&mut tcp, 2).expect("remaining staged read"),
        Some(b"c".to_vec())
    );
    assert_eq!(server.buffered_len(), 0);
}

/// Verifies delimiter framing consumes the delimiter and preserves following
/// bytes.
///
/// Inputs: two newline-delimited frames delivered in one byte chunk.
/// Output: test passes when each frame is returned without its delimiter.
/// Transformation: locks protocol fixtures for line-oriented codecs.
#[test]
fn vm_framing_fixture_reads_delimited_frames() {
    let (mut tcp, mut client, mut server) = connected_readers(64);

    client
        .write(&mut tcp, b"alpha\nbeta\n".to_vec())
        .expect("write lines");

    assert_eq!(
        server.read_until(&mut tcp, b'\n').expect("first line"),
        Some(b"alpha".to_vec())
    );
    assert_eq!(
        server.read_until(&mut tcp, b'\n').expect("second line"),
        Some(b"beta".to_vec())
    );
}

/// Verifies length-prefixed framing handles fragmented payload delivery.
///
/// Inputs: one u32 length-prefixed frame sent in prefix and payload chunks.
/// Output: test passes when the reader waits for the payload and returns only
/// the payload bytes.
/// Transformation: establishes the baseline fixture for binary protocols and
/// HTTP benchmark payload framing.
#[test]
fn vm_framing_fixture_reads_fragmented_length_prefixed_frames() {
    let (mut tcp, mut client, mut server) = connected_readers(64);

    client
        .write(&mut tcp, [0, 0, 0, 5, b'h', b'e'].to_vec())
        .expect("write prefix and partial");
    assert_eq!(
        server
            .read_length_prefixed(&mut tcp)
            .expect("pending payload"),
        None
    );
    assert_eq!(server.buffered_len(), 6);

    client.write(&mut tcp, b"llo".to_vec()).expect("write rest");
    assert_eq!(
        server
            .read_length_prefixed(&mut tcp)
            .expect("complete frame"),
        Some(b"hello".to_vec())
    );
}

/// Verifies length-prefixed writes use the same wire shape as reads.
///
/// Inputs: one payload written with `write_length_prefixed`.
/// Output: test passes when the peer reads the original payload.
/// Transformation: locks frame encode/decode symmetry for in-memory streams.
#[test]
fn vm_framing_fixture_writes_length_prefixed_frames() {
    let (mut tcp, mut client, mut server) = connected_readers(64);

    assert_eq!(
        client
            .write_length_prefixed(&mut tcp, b"hello".to_vec())
            .expect("write frame"),
        9
    );
    assert_eq!(
        server.read_length_prefixed(&mut tcp).expect("read frame"),
        Some(b"hello".to_vec())
    );
}

/// Verifies close while pending reports EOF when the peer closed writes.
///
/// Inputs: an incomplete exact read followed by peer write-side close.
/// Output: test passes when the pending frame reports `FramingEof`.
/// Transformation: distinguishes graceful protocol EOF from ordinary pending
/// readiness.
#[test]
fn vm_framing_fixture_reports_eof_for_half_closed_partial_frame() {
    let (mut tcp, mut client, mut server) = connected_readers(64);

    client
        .write(&mut tcp, b"abc".to_vec())
        .expect("write partial");
    assert_eq!(server.read_exact(&mut tcp, 5).expect("pending"), None);
    tcp.close_write(client.stream()).expect("close write");

    assert_eq!(
        server
            .read_exact(&mut tcp, 5)
            .expect_err("partial frame eof"),
        VmFramingError::FramingEof
    );
}

/// Verifies pending reads can be converted into deterministic timeouts.
///
/// Inputs: one empty stream and an elapsed timeout flag.
/// Output: test passes when timeout is reported without consuming state.
/// Transformation: models scheduler-driven timeout delivery through the VM.
#[test]
fn vm_framing_fixture_reports_timeout_for_pending_exact_read() {
    let (mut tcp, _client, mut server) = connected_readers(64);

    assert_eq!(
        server
            .read_exact_with_timeout(&mut tcp, 1, true)
            .expect_err("timeout"),
        VmFramingError::Timeout
    );
}

/// Verifies cancelled streams report cancellation through the framing layer.
///
/// Inputs: one server stream cancelled before read.
/// Output: test passes when the framing layer maps cancellation to a typed
/// result.
/// Transformation: keeps protocol readers from seeing raw TCP diagnostics.
#[test]
fn vm_framing_fixture_reports_cancelled_streams() {
    let (mut tcp, _client, mut server) = connected_readers(64);
    tcp.cancel_stream(server.stream()).expect("cancel server");

    assert_eq!(
        server.read_exact(&mut tcp, 1).expect_err("cancelled"),
        VmFramingError::Cancelled
    );
}

/// Verifies bounded buffers reject oversized pending frames.
///
/// Inputs: one reader with four-byte capacity and five bytes of incoming data.
/// Output: test passes when the frame reports overflow.
/// Transformation: proves backpressure-facing protocol buffers do not grow
/// without bound.
#[test]
fn vm_framing_fixture_rejects_bounded_buffer_overflow() {
    let (mut tcp, mut client, mut server) = connected_readers(4);

    client.write(&mut tcp, b"abcde".to_vec()).expect("write");

    assert_eq!(
        server.read_until(&mut tcp, b'\n').expect_err("overflow"),
        VmFramingError::FramingOverflow
    );
}

/// Verifies peer inbox backpressure is exposed as typed framing pressure.
///
/// Inputs: one server stream with limited inbox capacity.
/// Output: test passes when a write beyond peer capacity returns
/// `BackpressureExceeded`.
/// Transformation: prevents protocol writers from assuming infinite buffering.
#[test]
fn vm_framing_fixture_reports_backpressure_from_peer_inbox() {
    let (mut tcp, mut client, server) = connected_readers(64);
    tcp.set_stream_inbox_limit(server.stream(), 3)
        .expect("set limit");

    assert_eq!(client.write(&mut tcp, b"abc".to_vec()).expect("fill"), 3);
    assert_eq!(
        client
            .write(&mut tcp, b"d".to_vec())
            .expect_err("backpressure"),
        VmFramingError::BackpressureExceeded
    );
}

/// Verifies closed streams report the closed framing outcome.
///
/// Inputs: one reader whose stream is closed before a pending read completes.
/// Output: test passes when the framing layer reports `Closed`.
/// Transformation: distinguishes owner close from protocol EOF.
#[test]
fn vm_framing_fixture_reports_closed_reader_stream() {
    let (mut tcp, _client, mut server) = connected_readers(64);
    tcp.close_stream(server.stream()).expect("close server");

    assert_eq!(
        server.read_exact(&mut tcp, 1).expect_err("closed"),
        VmFramingError::Closed
    );
}
