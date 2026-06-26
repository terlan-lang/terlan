/// Derives the SafeNative backend module from a Terlan module path.
///
/// Inputs:
/// - `module`: source module path such as `std.data.Json`.
///
/// Output:
/// - Lower-snake SafeNative module name such as `std_data_json_safe_native`.
///
/// Transformation:
/// - Converts each path segment to lower snake case, joins segments with
///   underscores, and appends the SafeNative suffix.
pub fn module_path_to_safe_native_module(module: &str) -> String {
    let base = module
        .split('.')
        .filter(|segment| !segment.is_empty())
        .map(identifier_to_snake)
        .collect::<Vec<_>>()
        .join("_");
    format!("{base}_safe_native")
}

/// Converts one identifier segment to lower snake case.
///
/// Inputs:
/// - `segment`: module path segment in Terlan casing.
///
/// Output:
/// - Lower-snake representation.
///
/// Transformation:
/// - Inserts underscores before uppercase boundaries where needed and lowers
///   alphabetic characters.
pub fn identifier_to_snake(segment: &str) -> String {
    let mut out = String::new();
    let mut previous_was_lower_or_digit = false;
    for ch in segment.chars() {
        if ch.is_ascii_uppercase() {
            if previous_was_lower_or_digit && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            previous_was_lower_or_digit = false;
        } else if ch == '-' {
            if !out.ends_with('_') {
                out.push('_');
            }
            previous_was_lower_or_digit = false;
        } else {
            out.push(ch.to_ascii_lowercase());
            previous_was_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
    }
    out
}
