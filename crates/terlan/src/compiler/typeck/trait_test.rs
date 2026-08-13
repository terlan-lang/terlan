pub(super) use super::test_support::*;
pub(super) use super::*;

#[cfg(test)]
#[path = "trait_test/higher_kinded_and_imported_traits.rs"]
mod higher_kinded_and_imported_traits;
#[cfg(test)]
#[path = "trait_test/trait_declarations_and_methods.rs"]
mod trait_declarations_and_methods;
