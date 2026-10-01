use std::fmt;
use terlan_runtime_abi::NativeAdapterError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionError {
    ZeroTtl,
    Stale(String),
    EmptySnapshotIdentity,
    ExpiredSnapshot(String),
    DuplicateSnapshot(String),
    Resource(String),
    Identity(NativeAdapterError),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroTtl => f.write_str("HTTP session TTL must be greater than 0"),
            Self::Stale(id) => write!(f, "stale HTTP session `{id}`"),
            Self::EmptySnapshotIdentity => {
                f.write_str("HTTP session persistence snapshot id cannot be empty")
            }
            Self::ExpiredSnapshot(id) => {
                write!(f, "HTTP session persistence snapshot `{id}` is expired")
            }
            Self::DuplicateSnapshot(id) => write!(
                f,
                "HTTP session persistence snapshot `{id}` would overwrite live session"
            ),
            Self::Resource(message) => f.write_str(message),
            Self::Identity(error) => write!(f, "error[{}]: {}", error.code(), error.message()),
        }
    }
}

impl std::error::Error for SessionError {}
