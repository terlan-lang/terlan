//! TCP listener construction through maintained standard-library and socket2 APIs.
//! Registration, acceptance budgets, and scheduler placement remain caller-owned.

use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::net::{self as std_net, SocketAddr, ToSocketAddrs};

mod ready;
pub use ready::{TcpConnection, TcpIncoming};

/// Binds a reusable nonblocking listener using the established serving defaults.
pub fn bind_listener(host: &str, port: u16) -> Result<std_net::TcpListener, String> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|error| format!("error[vm.protocol_bind]: resolve {host}:{port}: {error}"))?;
    bind_addresses(host, port, addresses, bind_address)
}

fn bind_addresses(
    host: &str,
    port: u16,
    addresses: impl IntoIterator<Item = SocketAddr>,
    mut bind: impl FnMut(SocketAddr) -> io::Result<std_net::TcpListener>,
) -> Result<std_net::TcpListener, String> {
    let mut last_error = None;
    for address in addresses {
        match bind(address) {
            Ok(listener) => return Ok(listener),
            Err(error) => last_error = Some(error),
        }
    }
    Err(format!(
        "error[vm.protocol_bind]: bind {host}:{port}: {}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "host resolved to no addresses".to_string())
    ))
}

fn bind_address(address: SocketAddr) -> io::Result<std_net::TcpListener> {
    let socket = Socket::new(
        Domain::for_address(address),
        Type::STREAM,
        Some(Protocol::TCP),
    )?;
    #[cfg(unix)]
    {
        socket.set_reuse_address(true)?;
        socket.set_reuse_port(true)?;
    }
    socket.set_nonblocking(true)?;
    socket.bind(&address.into())?;
    socket.listen(1_024)?;
    Ok(socket.into())
}

#[cfg(test)]
#[path = "tcp_test.rs"]
mod tests;
