//! Production build-revision source classification shared with its regression tests.

use std::path::Path;

pub(crate) fn is_test_only_rust_source(path: &Path) -> bool {
    let in_test_directory = path.components().any(|component| {
        let component = component.as_os_str().to_string_lossy();
        component == "tests" || component.ends_with("_test") || component.ends_with("_tests")
    });
    let test_file = path.file_name().is_some_and(|name| {
        let name = name.to_string_lossy();
        name.ends_with("_test.rs")
            || name.ends_with("_test_support.rs")
            || name.ends_with("_test_fixture.rs")
    });
    in_test_directory || test_file
}

#[cfg(test)]
#[path = "source_revision_test.rs"]
mod tests;
