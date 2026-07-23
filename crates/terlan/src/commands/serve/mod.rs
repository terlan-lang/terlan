mod args;
#[cfg(test)]
mod channel_transport;
mod handler;
mod handler_cache;
mod hyper_server;
#[cfg(test)]
mod logging;
mod manifest;
mod response;
mod tls;
mod watch;
mod websocket;

#[cfg(test)]
#[path = "serve_test.rs"]
mod serve_test;
include!("mod_part_001.rs");
include!("mod_part_002.rs");
include!("mod_part_003.rs");
