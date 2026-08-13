//! VM-owned EPMD discovery and logical-node lifecycle.

pub(crate) mod bootstrap;
pub(crate) mod client;
pub(crate) mod lifecycle;
pub(crate) mod node_transport;
pub(crate) mod protocol;
pub(crate) mod state;
pub(crate) mod transport;

#[cfg(test)]
#[path = "epmd_test.rs"]
#[cfg(test)]
mod epmd_test;
