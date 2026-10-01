use super::*;
use std::io::{self, Read, Write};
use std::net::Shutdown;

#[test]
fn real_socket_receive_and_tls_half_close_preserve_read_half() -> io::Result<()> {
    use crate::tcp::TcpConnection;
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    struct Registered;
    impl WriteInterest<TcpConnection> for Registered {
        fn arm_writable(&mut self, _stream: &mut TcpConnection) -> io::Result<()> {
            Ok(())
        }
    }
    let listener = crate::tcp::bind_listener("127.0.0.1", 0).map_err(io::Error::other)?;
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    listener.set_nonblocking(false)?;
    let socket = TcpStream::connect(listener.local_addr()?)?;
    let (mut peer, _) = listener.accept()?;
    socket.set_nonblocking(true)?;
    peer.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut stream = ReadyStream::new(TcpConnection::new(socket)?, Registered);
    peer.write_all(b"abc")?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut received = 0;
    while received < 3 {
        let mut bytes = [0; 3];
        match stream.read(&mut bytes[..3 - received]) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(count) => {
                assert_eq!(&bytes[..count], &b"abc"[received..received + count]);
                received += count;
            }
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(1))
            }
            Err(error) => return Err(error),
        }
    }
    crate::tls_stream::ShutdownWrite::shutdown_write(&mut stream)?;
    assert_eq!(peer.read(&mut [0; 1])?, 0);
    peer.write_all(b"still readable")?;
    peer.shutdown(Shutdown::Write)?;
    let mut data = Vec::new();
    loop {
        let mut bytes = [0; 32];
        match stream.read(&mut bytes) {
            Ok(0) => break,
            Ok(count) => data.extend_from_slice(&bytes[..count]),
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(1))
            }
            Err(error) => return Err(error),
        }
    }
    assert_eq!(data, b"still readable");
    Ok(())
}
