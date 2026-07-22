use super::stream::VmHttpStreamState;
use super::{VmHttpStaticError, VmHttpStreamPlan};
use crate::runtime::vm::framing::VmInMemoryFrameReader;
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::tcp::{VmTcpRuntime, VmTcpStream, VmTcpWake};

#[test]
fn vm_http_response_stream_splits_and_partially_flushes_in_order() {
    let mut stream = VmHttpStreamPlan::new(3, 4).expect("plan").open_stream();

    assert_eq!(stream.enqueue(b"abcdefgh".to_vec()).expect("enqueue"), 3);
    assert_eq!(stream.flush_next().expect("flush"), Some(b"abc".to_vec()));
    assert_eq!(stream.flush_next().expect("flush"), Some(b"def".to_vec()));
    assert_eq!(
        stream.inspect(),
        super::stream::VmHttpStreamInfo {
            state: VmHttpStreamState::Open,
            pending_writes: 1,
            max_pending_writes: 4,
            emitted_chunks: 2,
            emitted_bytes: 6,
        }
    );
    assert_eq!(stream.flush_next().expect("flush"), Some(b"gh".to_vec()));
}

#[test]
fn vm_http_response_stream_applies_atomic_backpressure() {
    let mut stream = VmHttpStreamPlan::new(2, 2).expect("plan").open_stream();

    assert_eq!(
        stream.enqueue(b"abcde".to_vec()).expect_err("three chunks"),
        VmHttpStaticError::StreamBackpressure
    );
    assert_eq!(stream.inspect().pending_writes, 0);
    assert_eq!(
        stream.enqueue(Vec::new()).expect_err("empty chunk"),
        VmHttpStaticError::InvalidStreamChunk
    );
    assert_eq!(stream.enqueue(b"abcd".to_vec()).expect("two chunks"), 2);
    assert_eq!(
        stream.enqueue(b"x".to_vec()).expect_err("queue full"),
        VmHttpStaticError::StreamBackpressure
    );
}

#[test]
fn vm_http_response_stream_finishes_and_aborts_with_stable_states() {
    let mut finishing = VmHttpStreamPlan::new(4, 2).expect("plan").open_stream();
    finishing.enqueue(b"hello".to_vec()).expect("enqueue");
    finishing.finish().expect("finish");
    assert_eq!(finishing.inspect().state, VmHttpStreamState::Finishing);
    assert_eq!(
        finishing.enqueue(b"late".to_vec()).expect_err("closed"),
        VmHttpStaticError::StreamClosed
    );
    assert_eq!(
        finishing.flush_next().expect("flush"),
        Some(b"hell".to_vec())
    );
    assert_eq!(finishing.flush_next().expect("flush"), Some(b"o".to_vec()));
    assert_eq!(finishing.inspect().state, VmHttpStreamState::Complete);
    finishing.finish().expect("idempotent finish");
    assert_eq!(
        finishing.abort().expect_err("complete stream"),
        VmHttpStaticError::StreamClosed
    );

    let mut aborted = VmHttpStreamPlan::new(2, 3).expect("plan").open_stream();
    aborted.enqueue(b"abcd".to_vec()).expect("enqueue");
    assert_eq!(aborted.abort().expect("abort"), 2);
    assert_eq!(aborted.inspect().state, VmHttpStreamState::Aborted);
    assert_eq!(aborted.inspect().pending_writes, 0);
    assert_eq!(
        aborted.flush_next().expect_err("aborted flush"),
        VmHttpStaticError::StreamAborted
    );
    assert_eq!(
        aborted.finish().expect_err("aborted finish"),
        VmHttpStaticError::StreamAborted
    );
}

#[test]
fn vm_http_response_stream_flushes_to_tcp_in_order() {
    let (mut tcp, mut writer, client) = connected_writer(16);
    let mut stream = VmHttpStreamPlan::new(3, 3).expect("plan").open_stream();
    stream.enqueue(b"abcdef".to_vec()).expect("enqueue");
    stream.finish().expect("finish");
    let process = VmProcessId::from_raw_for_test(41);

    assert_eq!(
        stream
            .flush_next_to_tcp(&mut writer, &mut tcp, process)
            .expect("first flush"),
        super::stream::VmHttpStreamTcpFlush::Written {
            bytes: 3,
            state: VmHttpStreamState::Finishing,
        }
    );
    assert_eq!(
        tcp.receive(client, 3).expect("first receive"),
        Some(b"abc".to_vec())
    );
    assert_eq!(
        stream
            .flush_next_to_tcp(&mut writer, &mut tcp, process)
            .expect("second flush"),
        super::stream::VmHttpStreamTcpFlush::Written {
            bytes: 3,
            state: VmHttpStreamState::Complete,
        }
    );
    assert_eq!(
        tcp.receive(client, 3).expect("second receive"),
        Some(b"def".to_vec())
    );
    assert_eq!(
        stream
            .flush_next_to_tcp(&mut writer, &mut tcp, process)
            .expect("complete poll"),
        super::stream::VmHttpStreamTcpFlush::Complete
    );
}

#[test]
fn vm_http_response_stream_parks_and_retries_tcp_backpressure() {
    let (mut tcp, mut writer, client) = connected_writer(3);
    let mut stream = VmHttpStreamPlan::new(3, 3).expect("plan").open_stream();
    stream.enqueue(b"abcdef".to_vec()).expect("enqueue");
    stream.finish().expect("finish");
    let process = VmProcessId::from_raw_for_test(42);

    stream
        .flush_next_to_tcp(&mut writer, &mut tcp, process)
        .expect("first flush");
    assert_eq!(
        stream
            .flush_next_to_tcp(&mut writer, &mut tcp, process)
            .expect("backpressure poll"),
        super::stream::VmHttpStreamTcpFlush::Parked
    );
    assert_eq!(stream.inspect().pending_writes, 1);
    assert_eq!(stream.inspect().emitted_chunks, 1);
    assert_eq!(
        tcp.inspect_stream(client)
            .expect("client info")
            .waiting_writers,
        1
    );

    let (received, wakeups) = tcp.receive_with_wakeups(client, 3).expect("drain peer");
    assert_eq!(received, Some(b"abc".to_vec()));
    assert_eq!(
        wakeups,
        vec![VmTcpWake::Write {
            process,
            stream: writer.stream(),
        }]
    );
    assert_eq!(
        stream
            .flush_next_to_tcp(&mut writer, &mut tcp, process)
            .expect("retry flush"),
        super::stream::VmHttpStreamTcpFlush::Written {
            bytes: 3,
            state: VmHttpStreamState::Complete,
        }
    );
}

#[test]
fn vm_http_response_stream_aborts_on_terminal_tcp_failures() {
    let (mut closed_tcp, mut closed_writer, closed_client) = connected_writer(8);
    let mut closed = VmHttpStreamPlan::new(4, 2).expect("plan").open_stream();
    closed.enqueue(b"data".to_vec()).expect("enqueue");
    closed_tcp.close_stream(closed_client).expect("close peer");
    assert_eq!(
        closed
            .flush_next_to_tcp(
                &mut closed_writer,
                &mut closed_tcp,
                VmProcessId::from_raw_for_test(43),
            )
            .expect_err("closed peer"),
        VmHttpStaticError::StreamTransportClosed
    );
    assert_eq!(closed.inspect().state, VmHttpStreamState::Aborted);
    assert_eq!(closed.inspect().pending_writes, 0);

    let (mut cancelled_tcp, mut cancelled_writer, cancelled_client) = connected_writer(8);
    let mut cancelled = VmHttpStreamPlan::new(4, 2).expect("plan").open_stream();
    cancelled.enqueue(b"data".to_vec()).expect("enqueue");
    cancelled_tcp
        .cancel_stream(cancelled_client)
        .expect("cancel peer");
    assert_eq!(
        cancelled
            .flush_next_to_tcp(
                &mut cancelled_writer,
                &mut cancelled_tcp,
                VmProcessId::from_raw_for_test(44),
            )
            .expect_err("cancelled peer"),
        VmHttpStaticError::StreamTransportCancelled
    );
    assert_eq!(cancelled.inspect().state, VmHttpStreamState::Aborted);
    assert_eq!(cancelled.inspect().pending_writes, 0);
}

fn connected_writer(inbox_limit: usize) -> (VmTcpRuntime, VmInMemoryFrameReader, VmTcpStream) {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http-stream-test").expect("listen");
    let client = tcp.connect("http-stream-test", "client").expect("connect");
    let server = tcp
        .accept(listener, "server")
        .expect("accept")
        .expect("server stream");
    tcp.set_stream_inbox_limit(client, inbox_limit)
        .expect("set peer limit");
    let writer = VmInMemoryFrameReader::new(server, 64).expect("writer");
    (tcp, writer, client)
}
