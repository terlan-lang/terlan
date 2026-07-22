use super::super::{
    packet::{VmDecodedPacket, VmPacketMode, VmPacketOptions},
    tcp::VmTcpRuntime,
};
use super::VmInMemoryFrameReader;

#[test]
fn packet_decoder_preserves_fragmented_stream_state_and_trailing_frame() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("packet-fixture").expect("listen");
    let client = tcp.connect("packet-fixture", "client").expect("connect");
    let server = tcp
        .accept(listener, "server")
        .expect("accept")
        .expect("server stream");
    let mut writer = VmInMemoryFrameReader::new(client, 64).expect("writer");
    let mut reader = VmInMemoryFrameReader::new(server, 64).expect("reader");

    writer
        .write(&mut tcp, b"\x00\x05he".to_vec())
        .expect("partial packet");
    assert_eq!(
        reader
            .read_packet(&mut tcp, VmPacketMode::Length2, VmPacketOptions::default())
            .expect("pending packet"),
        None
    );
    assert_eq!(reader.buffered_len(), 4);

    writer
        .write(&mut tcp, b"llo\x00\x03bye".to_vec())
        .expect("remaining packets");
    assert_eq!(
        reader
            .read_packet(&mut tcp, VmPacketMode::Length2, VmPacketOptions::default())
            .expect("first packet"),
        Some(VmDecodedPacket::Bytes(b"hello".to_vec()))
    );
    assert_eq!(
        reader
            .read_packet(&mut tcp, VmPacketMode::Length2, VmPacketOptions::default())
            .expect("second packet"),
        Some(VmDecodedPacket::Bytes(b"bye".to_vec()))
    );
}
