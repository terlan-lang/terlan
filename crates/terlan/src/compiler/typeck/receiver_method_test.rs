pub(super) use super::test_support::*;
pub(super) use super::*;
pub(super) use crate::terlan_syntax::parse_module_as_syntax_output;

#[cfg(test)]
#[path = "receiver_method_test/dispatch_and_identity.rs"]
mod dispatch_and_identity;
#[cfg(test)]
#[path = "receiver_method_test/resolution_and_defaults.rs"]
mod resolution_and_defaults;
