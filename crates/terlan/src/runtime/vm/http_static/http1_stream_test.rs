use super::http1_stream::{VmHttp1StreamPart, VmHttp1StreamState, VmHttp1StreamTcpFlush};
use super::{VmHttpStaticError, VmHttpStreamPlan};
use crate::runtime::vm::framing::VmInMemoryFrameReader;
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::tcp::{VmTcpRuntime, VmTcpStream, VmTcpWake};

#[test]
fn vm_http1_response_stream_writes_head_chunks_and_end_in_order() {
    let (mut tcp, mut writer, client) = connected_writer(1024);
    let mut stream = http1_stream(3);
    stream.enqueue(b"abcdef".to_vec()).expect("enqueue");
    stream.finish().expect("finish");
    let process = VmProcessId::from_raw_for_test(51);

    assert_written(
        &mut stream,
        &mut writer,
        &mut tcp,
        process,
        VmHttp1StreamPart::Head,
    );
    let head = tcp.receive(client, 1024).expect("head read").expect("head");
    assert!(String::from_utf8(head)
        .expect("head text")
        .contains("Transfer-Encoding: chunked\r\n"));

    assert_written(
        &mut stream,
        &mut writer,
        &mut tcp,
        process,
        VmHttp1StreamPart::Chunk,
    );
    assert_eq!(
        tcp.receive(client, 1024).expect("chunk one"),
        Some(b"3\r\nabc\r\n".to_vec())
    );
    assert_written(
        &mut stream,
        &mut writer,
        &mut tcp,
        process,
        VmHttp1StreamPart::Chunk,
    );
    assert_eq!(
        tcp.receive(client, 1024).expect("chunk two"),
        Some(b"3\r\ndef\r\n".to_vec())
    );
    assert_eq!(stream.inspect().state, VmHttp1StreamState::Finalizing);

    assert_written(
        &mut stream,
        &mut writer,
        &mut tcp,
        process,
        VmHttp1StreamPart::End,
    );
    assert_eq!(
        tcp.receive(client, 1024).expect("end"),
        Some(b"0\r\n\r\n".to_vec())
    );
    assert_eq!(stream.inspect().state, VmHttp1StreamState::Complete);
    assert_eq!(stream.inspect().body.emitted_bytes, 6);
    assert_eq!(
        stream
            .flush_next_to_tcp(&mut writer, &mut tcp, process)
            .expect("complete poll"),
        VmHttp1StreamTcpFlush::Complete
    );
}

#[test]
fn vm_http1_response_stream_preserves_chunk_during_tcp_backpressure() {
    let (mut tcp, mut writer, client) = connected_writer(1024);
    let mut stream = http1_stream(3);
    stream.enqueue(b"abcdef".to_vec()).expect("enqueue");
    stream.finish().expect("finish");
    let process = VmProcessId::from_raw_for_test(52);

    assert_written(
        &mut stream,
        &mut writer,
        &mut tcp,
        process,
        VmHttp1StreamPart::Head,
    );
    tcp.receive(client, 1024).expect("drain head");
    tcp.set_stream_inbox_limit(client, 8)
        .expect("chunk-sized inbox");
    assert_written(
        &mut stream,
        &mut writer,
        &mut tcp,
        process,
        VmHttp1StreamPart::Chunk,
    );
    assert_eq!(
        stream
            .flush_next_to_tcp(&mut writer, &mut tcp, process)
            .expect("parked chunk"),
        VmHttp1StreamTcpFlush::Parked {
            part: VmHttp1StreamPart::Chunk,
        }
    );
    assert_eq!(stream.inspect().body.pending_writes, 1);
    assert_eq!(stream.inspect().body.emitted_chunks, 1);

    let (received, wakeups) = tcp.receive_with_wakeups(client, 8).expect("drain chunk");
    assert_eq!(received, Some(b"3\r\nabc\r\n".to_vec()));
    assert_eq!(
        wakeups,
        vec![VmTcpWake::Write {
            process,
            stream: writer.stream(),
        }]
    );
    assert_written(
        &mut stream,
        &mut writer,
        &mut tcp,
        process,
        VmHttp1StreamPart::Chunk,
    );
    assert_eq!(stream.inspect().body.pending_writes, 0);
}

#[test]
fn vm_http1_response_stream_rejects_invalid_metadata_and_terminal_races() {
    let invalid = ::http::Response::builder()
        .status(200)
        .header(::http::header::CONTENT_LENGTH, "4")
        .body(())
        .expect("response");
    assert_eq!(
        VmHttpStreamPlan::new(4, 2)
            .expect("plan")
            .open_http1_stream(invalid, false)
            .expect_err("conflicting metadata"),
        VmHttpStaticError::InvalidStreamResponse
    );

    let (mut tcp, mut writer, client) = connected_writer(1024);
    let mut stream = http1_stream(4);
    stream.finish().expect("finish empty body");
    let process = VmProcessId::from_raw_for_test(53);
    assert_written(
        &mut stream,
        &mut writer,
        &mut tcp,
        process,
        VmHttp1StreamPart::Head,
    );
    tcp.receive(client, 1024).expect("drain head");
    assert_eq!(stream.inspect().state, VmHttp1StreamState::Finalizing);
    assert_eq!(stream.abort().expect("abort before end"), 0);
    assert_eq!(stream.inspect().state, VmHttp1StreamState::Aborted);
    assert_eq!(
        stream
            .flush_next_to_tcp(&mut writer, &mut tcp, process)
            .expect_err("aborted response"),
        VmHttpStaticError::StreamAborted
    );

    let (mut closed_tcp, mut closed_writer, closed_client) = connected_writer(1024);
    let mut closed = http1_stream(4);
    closed_tcp.close_stream(closed_client).expect("close peer");
    assert_eq!(
        closed
            .flush_next_to_tcp(
                &mut closed_writer,
                &mut closed_tcp,
                VmProcessId::from_raw_for_test(54),
            )
            .expect_err("closed head write"),
        VmHttpStaticError::StreamTransportClosed
    );
    assert_eq!(closed.inspect().state, VmHttp1StreamState::Aborted);
}

fn http1_stream(chunk_size: usize) -> super::http1_stream::VmHttp1ResponseStream {
    let response = ::http::Response::builder()
        .status(200)
        .header(::http::header::CONTENT_TYPE, "text/plain")
        .body(())
        .expect("response");
    VmHttpStreamPlan::new(chunk_size, 4)
        .expect("plan")
        .open_http1_stream(response, false)
        .expect("HTTP stream")
}

fn assert_written(
    stream: &mut super::http1_stream::VmHttp1ResponseStream,
    writer: &mut VmInMemoryFrameReader,
    tcp: &mut VmTcpRuntime,
    process: VmProcessId,
    part: VmHttp1StreamPart,
) {
    assert!(matches!(
        stream
            .flush_next_to_tcp(writer, tcp, process)
            .expect("flush"),
        VmHttp1StreamTcpFlush::Written {
            part: actual,
            bytes: 1..,
            ..
        } if actual == part
    ));
}

fn connected_writer(inbox_limit: usize) -> (VmTcpRuntime, VmInMemoryFrameReader, VmTcpStream) {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http1-stream-test").expect("listen");
    let client = tcp.connect("http1-stream-test", "client").expect("connect");
    let server = tcp
        .accept(listener, "server")
        .expect("accept")
        .expect("server stream");
    tcp.set_stream_inbox_limit(client, inbox_limit)
        .expect("set peer limit");
    let writer = VmInMemoryFrameReader::new(server, 64).expect("writer");
    (tcp, writer, client)
}
