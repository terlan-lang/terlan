use tungstenite::handshake::derive_accept_key;
#[cfg(test)]
use tungstenite::protocol::{Message, Role, WebSocket};

use super::native_callable::VmNativeCallableRef;
#[cfg(test)]
use super::tcp::{VmTcpRuntime, VmTcpStream};
#[cfg(test)]
use super::tls::VmTlsTcpServerStream;
#[cfg(test)]
pub(crate) use memory::{VmAccountedWebSocketInboundQueue, VmAccountedWebSocketQueueError};
pub(crate) use memory::{VmWebSocketInboundQueue, VmWebSocketInboundQueueInfo};
pub(crate) use websocket_live_session::VmWebSocketLiveSession;

#[cfg(test)]
use std::{
    collections::HashMap,
    io::{Cursor, Read, Write},
};

#[path = "websocket/memory.rs"]
mod memory;
#[path = "websocket_live_session.rs"]
mod websocket_live_session;

#[cfg(test)]
#[path = "websocket/memory_test.rs"]
#[cfg(test)]
mod memory_test;

#[cfg(test)]
#[path = "websocket_test.rs"]
#[cfg(test)]
mod websocket_test;

#[path = "websocket/state.rs"]
mod state;

pub(crate) use state::*;
