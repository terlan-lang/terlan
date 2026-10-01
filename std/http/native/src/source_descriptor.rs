//! Validation of executed std.http values, never compiler syntax or builder names.

use terlan_runtime_abi::{DescriptorValue, DescriptorView, NativeAdapterError};

mod channels;
mod execution;
mod response;
mod restoration;
mod router;
mod transition;
pub use channels::{sse_endpoint, websocket_endpoint};
pub(crate) use execution::middleware_result;
pub use execution::HandlerPipeline;
pub use response::{cached_response, response, SourceResponse, SourceResponseBody};
pub use restoration::restoration_identity;
pub use router::{router, Route, RouteTarget, Router};
pub use transition::{paired_transition, PairedTransition};

#[cfg(test)]
#[path = "source_descriptor_test.rs"]
mod tests;

type Result<T> = std::result::Result<T, NativeAdapterError>;

fn error(message: impl std::fmt::Display) -> NativeAdapterError {
    NativeAdapterError::new("http.descriptor", message.to_string(), 0)
}

fn record<'a, V: DescriptorValue, const N: usize>(
    value: &'a V,
    expected_name: &str,
    names: [&str; N],
) -> Result<[&'a V; N]> {
    let DescriptorView::Record(name, fields) = value.descriptor_view() else {
        return Err(error(format!("expected {expected_name} record")));
    };
    if name != expected_name || fields.len() != N {
        return Err(error(format!("invalid {expected_name} record shape")));
    }
    named_fields(fields, &names)?
        .try_into()
        .map_err(|_| error("invalid record field count"))
}

fn named_fields<'a, V>(fields: &'a [(String, V)], names: &[&str]) -> Result<Vec<&'a V>> {
    names
        .iter()
        .map(|name| {
            let mut matching = fields.iter().filter(|(key, _)| key == name);
            match (matching.next(), matching.next()) {
                (Some((_, value)), None) => Ok(value),
                _ => Err(error(format!("missing or duplicate field `{name}`"))),
            }
        })
        .collect()
}

fn list<V: DescriptorValue>(value: &V) -> Result<&[V]> {
    match value.descriptor_view() {
        DescriptorView::List(values) => Ok(values),
        _ => Err(error("expected List")),
    }
}

fn text<V: DescriptorValue>(value: &V) -> Result<String> {
    match value.descriptor_view() {
        DescriptorView::String(value) => Ok(value.to_owned()),
        _ => Err(error("expected String")),
    }
}

fn positive<V: DescriptorValue>(value: &V) -> Result<usize> {
    usize::try_from(positive_u64(value)?).map_err(|_| error("limit exceeds platform capacity"))
}

fn positive_u64<V: DescriptorValue>(value: &V) -> Result<u64> {
    match value.descriptor_view() {
        DescriptorView::Int(value) if value > 0 => Ok(value as u64),
        _ => Err(error("expected positive Int limit")),
    }
}

fn variant<'a, V: DescriptorValue>(
    value: &'a V,
    schemas: &[(&str, &[&str])],
) -> Result<(&'a str, Vec<&'a V>)> {
    let DescriptorView::Record(name, fields) = value.descriptor_view() else {
        return Err(error("expected named constructor"));
    };
    let (_, names) = schemas
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .ok_or_else(|| error(format!("unknown constructor `{name}`")))?;
    if fields.len() != names.len() {
        return Err(error(format!("invalid {name} constructor shape")));
    }
    Ok((name, named_fields(fields, names)?))
}
