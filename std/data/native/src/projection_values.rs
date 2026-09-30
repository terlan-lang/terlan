//! One-to-one owned projections; names match the package's Terlan records.

use crate::{NestedStringFieldRow, RequiredFieldProjection, RequiredFieldRow, StringFieldRow};
use terlan_runtime_abi::{native_record, NativeValue};

impl From<StringFieldRow> for NativeValue {
    fn from(row: StringFieldRow) -> Self {
        let StringFieldRow { object, values } = row;
        native_record!(StringFieldRow, { object, values })
    }
}

impl From<NestedStringFieldRow> for NativeValue {
    fn from(row: NestedStringFieldRow) -> Self {
        let NestedStringFieldRow {
            object,
            values,
            child_array,
            children,
        } = row;
        native_record!(NestedStringFieldRow, { object, values, child_array, children })
    }
}

impl From<RequiredFieldProjection> for NativeValue {
    fn from(projection: RequiredFieldProjection) -> Self {
        let RequiredFieldProjection {
            strings,
            ints,
            array_lengths,
        } = projection;
        native_record!(RequiredFieldProjection, { strings, ints, array_lengths })
    }
}

impl From<RequiredFieldRow> for NativeValue {
    fn from(row: RequiredFieldRow) -> Self {
        let RequiredFieldRow {
            strings,
            ints,
            bools,
        } = row;
        native_record!(RequiredFieldRow, { strings, ints, bools })
    }
}

#[cfg(test)]
#[path = "projection_values_test.rs"]
mod tests;
