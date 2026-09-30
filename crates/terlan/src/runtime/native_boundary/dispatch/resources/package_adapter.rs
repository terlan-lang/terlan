//! Host value/handle adaptation, with package behavior supplied by callbacks.

use super::{DispatchError, NativeBoundaryBridgeValue, ResourceStore};
use terlan_runtime_abi::{NativeResourceAdapter, NativeResourceStore, NativeResourceValue};

pub(super) fn dispatch<R: Clone>(
    adapter: &NativeResourceAdapter<R>,
    store: &mut ResourceStore,
    owner: u64,
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> Result<NativeBoundaryBridgeValue, DispatchError>
where
    ResourceStore: NativeResourceStore<R>,
{
    let values = args
        .iter()
        .map(|arg| match arg {
            NativeBoundaryBridgeValue::Handle(handle) => Ok(NativeResourceValue::Resource(*handle)),
            value => super::decode_owned_bridge_value(operation, value)
                .and_then(|value| super::super::value_packages::to_native(&value))
                .map(NativeResourceValue::Value),
        })
        .collect::<Result<Vec<_>, _>>()?;
    match adapter.call(store, owner, operation, &values)? {
        NativeResourceValue::Resource(handle) => Ok(NativeBoundaryBridgeValue::Handle(handle)),
        NativeResourceValue::Value(value) => super::encode_bridge_result(
            store,
            owner,
            super::super::value_packages::from_native(value),
        ),
    }
}
