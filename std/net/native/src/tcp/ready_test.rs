use super::*;
use std::time::Duration;

#[test]
fn tcp_package_accepts_registered_streams_and_preserves_half_close() -> io::Result<()> {
    let listener = crate::tcp::bind_listener("127.0.0.1", 0).map_err(io::Error::other)?;
    let address = listener.local_addr()?;
    let mut incoming = TcpIncoming::new(listener)?;
    assert!(matches!(incoming.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock));
    let mut poll = mio::Poll::new()?;
    incoming.register(poll.registry(), Token(10), Interest::READABLE)?;
    incoming.reregister(poll.registry(), Token(11), Interest::READABLE)?;
    let mut client = net::TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    client.set_read_timeout(Some(Duration::from_secs(2)))?;
    client.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut events = mio::Events::with_capacity(8);
    poll.poll(&mut events, Some(Duration::from_secs(2)))?;
    assert!(events
        .iter()
        .any(|event| event.token() == Token(11) && event.is_readable()));
    let mut stream = incoming.accept()?;
    incoming.deregister(poll.registry())?;
    stream.register(poll.registry(), Token(12), Interest::READABLE)?;
    assert_eq!(
        stream.read(&mut [0; 1]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    client.write_all(b"a")?;
    poll.poll(&mut events, Some(Duration::from_secs(2)))?;
    assert!(events
        .iter()
        .any(|event| event.token() == Token(12) && event.is_readable()));
    let mut bytes = [0; 1];
    assert_eq!(stream.read(&mut bytes)?, 1);
    assert_eq!(&bytes, b"a");
    stream.reregister(
        poll.registry(),
        Token(13),
        Interest::READABLE.add(Interest::WRITABLE),
    )?;
    let written = stream.write_vectored(&[IoSlice::new(b"b"), IoSlice::new(b"c")])?;
    assert!(written > 0 && written <= 2);
    stream.write_all(&b"bc"[written..])?;
    stream.flush()?;
    stream.shutdown_write()?;
    let mut response = Vec::new();
    client.read_to_end(&mut response)?;
    assert_eq!(response, b"bc");
    stream.deregister(poll.registry())?;
    Ok(())
}
