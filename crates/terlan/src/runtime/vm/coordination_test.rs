pub(super) use super::{VmClusterMembership, VmClusterNodeState, VmCoordinationProfile};

#[cfg(test)]
#[path = "coordination_test/membership_lifecycle.rs"]
mod membership_lifecycle;
use membership_lifecycle::*;
#[cfg(test)]
#[path = "coordination_test/peer_protocol.rs"]
mod peer_protocol;
