//! Rust-owned indexed vector resource for `std.native.collections.Vector`.
//!
//! This module owns the target-native storage semantics for Terlan vectors
//! before a NIF or worker transport is attached. Values are stored in the
//! bridge-neutral term shape so BEAM handlers can keep opaque handles while the
//! Rust side owns indexed mutation.

use crate::terlan_safenative::dispatch::SafeNativeBridgeValue;

/// Rust-owned indexed collection behind `std.native.collections.Vector`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativeVector {
    values: Vec<SafeNativeBridgeValue>,
}

impl NativeVector {
    /// Builds an empty native vector resource.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - Empty `NativeVector`.
    ///
    /// Transformation:
    /// - Initializes an owned Rust `Vec` without exposing its allocation across
    ///   the bridge boundary.
    pub fn new() -> Self {
        Self { values: Vec::new() }
    }

    /// Builds a native vector from bridge-neutral values.
    ///
    /// Inputs:
    /// - `values`: values decoded by the SafeNative bridge.
    ///
    /// Output:
    /// - `NativeVector` containing the values in the same order.
    ///
    /// Transformation:
    /// - Moves the bridge values into Rust-owned indexed storage.
    pub fn from_values(values: Vec<SafeNativeBridgeValue>) -> Self {
        Self { values }
    }

    /// Returns the vector length.
    ///
    /// Inputs:
    /// - `self`: native vector resource.
    ///
    /// Output:
    /// - Number of stored values as `i64` for Terlan `Int`.
    ///
    /// Transformation:
    /// - Observes Rust vector length without mutating storage.
    pub fn length(&self) -> Result<i64, VectorError> {
        i64::try_from(self.values.len()).map_err(|_| {
            VectorError::new(
                "vector.length_overflow",
                "Native vector length does not fit in Terlan Int.",
            )
        })
    }

    /// Returns the value at an index.
    ///
    /// Inputs:
    /// - `self`: native vector resource.
    /// - `index`: Terlan `Int` index.
    ///
    /// Output:
    /// - Cloned bridge value at that index.
    /// - `VectorError` when the index is negative or outside the vector.
    ///
    /// Transformation:
    /// - Converts the Terlan index to a Rust `usize` and clones the selected
    ///   bridge-neutral value for return across the runtime boundary.
    pub fn get_at(&self, index: i64) -> Result<SafeNativeBridgeValue, VectorError> {
        let index = checked_index(index)?;
        self.values
            .get(index)
            .cloned()
            .ok_or_else(|| index_error(index, self.values.len()))
    }

    /// Replaces the value at an index.
    ///
    /// Inputs:
    /// - `self`: native vector resource.
    /// - `index`: Terlan `Int` index.
    /// - `value`: bridge-neutral value to store.
    ///
    /// Output:
    /// - `Ok(())` when the value is replaced.
    /// - `VectorError` when the index is negative or outside the vector.
    ///
    /// Transformation:
    /// - Mutates the Rust-owned vector slot in place while preserving the same
    ///   outer resource handle.
    pub fn set_at(&mut self, index: i64, value: SafeNativeBridgeValue) -> Result<(), VectorError> {
        let index = checked_index(index)?;
        let len = self.values.len();
        match self.values.get_mut(index) {
            Some(slot) => {
                *slot = value;
                Ok(())
            }
            None => Err(index_error(index, len)),
        }
    }

    /// Swaps two values by index.
    ///
    /// Inputs:
    /// - `self`: native vector resource.
    /// - `left`: first Terlan `Int` index.
    /// - `right`: second Terlan `Int` index.
    ///
    /// Output:
    /// - `Ok(())` when both indexes are valid and values are swapped.
    /// - `VectorError` when either index is negative or outside the vector.
    ///
    /// Transformation:
    /// - Validates both indexes before mutating the Rust vector in place.
    pub fn swap(&mut self, left: i64, right: i64) -> Result<(), VectorError> {
        let left = checked_index(left)?;
        let right = checked_index(right)?;
        let len = self.values.len();
        if left >= len {
            return Err(index_error(left, len));
        }
        if right >= len {
            return Err(index_error(right, len));
        }
        self.values.swap(left, right);
        Ok(())
    }

    /// Appends a value to the vector.
    ///
    /// Inputs:
    /// - `self`: native vector resource.
    /// - `value`: bridge-neutral value to append.
    ///
    /// Output:
    /// - None.
    ///
    /// Transformation:
    /// - Pushes the value into Rust-owned indexed storage.
    pub fn push(&mut self, value: SafeNativeBridgeValue) {
        self.values.push(value);
    }

    /// Returns all values as a bridge-neutral list.
    ///
    /// Inputs:
    /// - `self`: native vector resource.
    ///
    /// Output:
    /// - Cloned values in vector order.
    ///
    /// Transformation:
    /// - Copies bridge-neutral values out of Rust storage for list conversion
    ///   without exposing the vector allocation itself.
    pub fn to_values(&self) -> Vec<SafeNativeBridgeValue> {
        self.values.clone()
    }
}

/// Creates an empty native vector resource.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Empty `NativeVector`.
///
/// Transformation:
/// - Delegates to `NativeVector::new` as the manifest-visible SafeNative
///   adapter function for `std.native.collections.vector.new`.
pub fn new() -> NativeVector {
    NativeVector::new()
}

/// Creates a native vector from bridge-neutral values.
///
/// Inputs:
/// - `values`: list values decoded from Terlan.
///
/// Output:
/// - `NativeVector` containing the values in source order.
///
/// Transformation:
/// - Delegates to `NativeVector::from_values` as the manifest-visible
///   SafeNative adapter function for `std.native.collections.vector.from_list`.
pub fn from_list(values: Vec<SafeNativeBridgeValue>) -> NativeVector {
    NativeVector::from_values(values)
}

/// Returns the vector length.
///
/// Inputs:
/// - `vector`: native vector resource.
///
/// Output:
/// - Length as Terlan `Int`.
/// - `VectorError` if the length cannot fit into Terlan `Int`.
///
/// Transformation:
/// - Delegates to the resource method as the manifest-visible SafeNative
///   adapter function for `std.native.collections.vector.length`.
pub fn length(vector: &NativeVector) -> Result<i64, VectorError> {
    vector.length()
}

/// Returns the vector length using the source-level short receiver name.
///
/// Inputs:
/// - `vector`: native vector resource.
///
/// Output:
/// - Length as Terlan `Int`.
/// - `VectorError` if the length cannot fit into Terlan `Int`.
///
/// Transformation:
/// - Delegates to `length` so `std.native.collections.Vector.len` and
///   `length` share one Rust-backed implementation.
pub fn len(vector: &NativeVector) -> Result<i64, VectorError> {
    length(vector)
}

/// Returns a vector value by index.
///
/// Inputs:
/// - `vector`: native vector resource.
/// - `index`: Terlan `Int` index.
///
/// Output:
/// - Cloned bridge-neutral value at the index.
/// - `VectorError` for invalid indexes.
///
/// Transformation:
/// - Delegates to the resource method as the manifest-visible SafeNative
///   adapter function for `std.native.collections.vector.get_at`.
pub fn get_at(vector: &NativeVector, index: i64) -> Result<SafeNativeBridgeValue, VectorError> {
    vector.get_at(index)
}

/// Replaces a vector value by index.
///
/// Inputs:
/// - `vector`: native vector resource to mutate.
/// - `index`: Terlan `Int` index.
/// - `value`: bridge-neutral value to store.
///
/// Output:
/// - `Ok(())` when the vector is updated.
/// - `VectorError` for invalid indexes.
///
/// Transformation:
/// - Delegates to the resource method as the manifest-visible SafeNative
///   adapter function for `std.native.collections.vector.set_at`.
pub fn set_at(
    vector: &mut NativeVector,
    index: i64,
    value: SafeNativeBridgeValue,
) -> Result<(), VectorError> {
    vector.set_at(index, value)
}

/// Swaps two vector values by index.
///
/// Inputs:
/// - `vector`: native vector resource to mutate.
/// - `left`: first Terlan `Int` index.
/// - `right`: second Terlan `Int` index.
///
/// Output:
/// - `Ok(())` when both indexes are valid.
/// - `VectorError` for invalid indexes.
///
/// Transformation:
/// - Delegates to the resource method as the manifest-visible SafeNative
///   adapter function for `std.native.collections.vector.swap`.
pub fn swap(vector: &mut NativeVector, left: i64, right: i64) -> Result<(), VectorError> {
    vector.swap(left, right)
}

/// Appends one value to a native vector.
///
/// Inputs:
/// - `vector`: native vector resource to mutate.
/// - `value`: bridge-neutral value to append.
///
/// Output:
/// - None.
///
/// Transformation:
/// - Delegates to the resource method as the manifest-visible SafeNative
///   adapter function for `std.native.collections.vector.push`.
pub fn push(vector: &mut NativeVector, value: SafeNativeBridgeValue) {
    vector.push(value);
}

/// Converts a native vector into bridge-neutral list values.
///
/// Inputs:
/// - `vector`: native vector resource.
///
/// Output:
/// - Cloned values in vector order.
///
/// Transformation:
/// - Delegates to the resource method as the manifest-visible SafeNative
///   adapter function for `std.native.collections.vector.to_list`.
pub fn to_list(vector: &NativeVector) -> Vec<SafeNativeBridgeValue> {
    vector.to_values()
}

/// Stable error returned by native vector operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VectorError {
    code: &'static str,
    message: String,
}

impl VectorError {
    /// Builds a native vector error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    ///
    /// Output:
    /// - `VectorError` suitable for SafeNative dispatch.
    ///
    /// Transformation:
    /// - Stores stable fields without exposing Rust collection internals.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Returns the stable machine-readable error code.
    ///
    /// Inputs:
    /// - `self`: vector error.
    ///
    /// Output:
    /// - Static error code.
    ///
    /// Transformation:
    /// - Reads the code field without allocation or mutation.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the human-readable error message.
    ///
    /// Inputs:
    /// - `self`: vector error.
    ///
    /// Output:
    /// - Borrowed message text.
    ///
    /// Transformation:
    /// - Reads the message field without allocation or mutation.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Converts a Terlan index into a Rust vector index.
///
/// Inputs:
/// - `index`: Terlan `Int` index.
///
/// Output:
/// - `usize` index when non-negative and representable.
/// - `VectorError` for negative indexes.
///
/// Transformation:
/// - Applies Terlan's current zero-based positive indexing rule.
fn checked_index(index: i64) -> Result<usize, VectorError> {
    usize::try_from(index).map_err(|_| {
        VectorError::new(
            "vector.negative_index",
            format!("Native vector index {index} is negative."),
        )
    })
}

/// Builds a vector bounds error.
///
/// Inputs:
/// - `index`: requested Rust vector index.
/// - `len`: current vector length.
///
/// Output:
/// - `VectorError` with stable code `vector.index_out_of_bounds`.
///
/// Transformation:
/// - Converts failed indexed access into stable SafeNative diagnostics.
fn index_error(index: usize, len: usize) -> VectorError {
    VectorError::new(
        "vector.index_out_of_bounds",
        format!("Native vector index {index} is out of bounds for length {len}."),
    )
}

#[cfg(test)]
#[path = "vector_test.rs"]
mod vector_test;
