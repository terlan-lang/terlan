//! Persistent random operations over actor-owned, generation-checked resources.

use crate::terlan_native::random;
use crate::terlan_native_boundary::resource::{ResourceStore, ResourceValue};

use super::super::args::{
    dispatch_resource_error, expect_bridge_handle, expect_bridge_int, expect_bridge_list,
    unknown_operation,
};
use super::super::{DispatchError, NativeBoundaryBridgeValue as Value};

#[cfg(test)]
#[path = "random/random_test.rs"]
mod tests;

/// Runs the existing Rust RNG without exposing its state or mutating its input.
pub(super) fn dispatch(
    store: &mut ResourceStore,
    owner: u64,
    operation: &str,
    args: &[Value],
) -> Result<Value, DispatchError> {
    match operation {
        "std.random.random.seed" => {
            let generator =
                random::seed(expect_bridge_int(operation, args, 0)?).map_err(random_error)?;
            return insert(store, owner, generator);
        }
        "std.random.random.entropy" => return insert(store, owner, random::entropy()),
        _ => {}
    }
    let handle = expect_bridge_handle(operation, args, 0)?;
    let generator = store
        .random_generator(handle)
        .map_err(dispatch_resource_error)?;
    let (next, value) = match operation {
        "std.random.random.int" => {
            let (next, value) = random::int(generator);
            (next, Value::Int(value))
        }
        "std.random.random.bounded_int" => {
            let min = expect_bridge_int(operation, args, 1)?;
            let max = expect_bridge_int(operation, args, 2)?;
            let (next, value) = random::bounded_int(generator, min, max).map_err(random_error)?;
            (next, Value::Int(value))
        }
        "std.random.random.float" => {
            let (next, value) = random::float(generator);
            (next, Value::Float(value))
        }
        "std.random.random.bool" => {
            let (next, value) = random::bool(generator);
            (next, Value::Bool(value))
        }
        "std.random.random.choice" => {
            let values = expect_bridge_list(operation, args, 1)?;
            let (next, index) = random::choice(generator, values.len()).map_err(random_error)?;
            (next, values[index].clone())
        }
        "std.random.random.shuffle" => {
            let values = expect_bridge_list(operation, args, 1)?;
            let (next, indices) = random::shuffle(generator, values.len());
            (
                next,
                Value::List(
                    indices
                        .into_iter()
                        .map(|index| values[index].clone())
                        .collect(),
                ),
            )
        }
        "std.random.random.sample" => {
            let values = expect_bridge_list(operation, args, 1)?;
            let count = expect_bridge_int(operation, args, 2)?;
            let (next, indices) =
                random::sample(generator, values.len(), count).map_err(random_error)?;
            (
                next,
                Value::List(
                    indices
                        .into_iter()
                        .map(|index| values[index].clone())
                        .collect(),
                ),
            )
        }
        _ => return Err(unknown_operation(operation)),
    };
    Ok(Value::Tuple(vec![insert(store, owner, next)?, value]))
}

fn insert(
    store: &mut ResourceStore,
    owner: u64,
    value: random::Generator,
) -> Result<Value, DispatchError> {
    store
        .insert_for_owner(owner, ResourceValue::RandomGenerator(value))
        .map(Value::Handle)
        .map_err(dispatch_resource_error)
}

fn random_error(error: random::RandomError) -> DispatchError {
    DispatchError::new(error.code(), error.message(), error.offset())
}
