pub(super) use super::test_support::*;
pub(super) use super::*;

#[cfg(test)]
#[path = "diagnostic_test/arguments_and_constructors.rs"]
mod arguments_and_constructors;
#[cfg(test)]
#[path = "diagnostic_test/macros_sql_and_shapes.rs"]
mod macros_sql_and_shapes;
#[cfg(test)]
#[path = "diagnostic_test/variance_and_visibility.rs"]
mod variance_and_visibility;
