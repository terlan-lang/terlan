//! Compatibility import for the HTTP package's route declaration contract.
pub(crate) use terlan_http_native::route_pattern::{
    is_identifier, route_ambiguity_key, route_param_names, route_param_types, route_segments,
    typed_route_param_segment, validate_route_pattern,
};
