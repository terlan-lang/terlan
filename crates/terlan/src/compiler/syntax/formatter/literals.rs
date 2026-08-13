/// Renders a Float without collapsing integral values into Int syntax.
pub(super) fn format_float_literal(value: f64) -> String {
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1.0e-6..1.0e21).contains(&magnitude) {
        return format!("{value:e}");
    }
    let rendered = value.to_string();
    if rendered.contains(['.', 'e', 'E']) {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

#[cfg(test)]
#[path = "literals_test.rs"]
#[cfg(test)]
mod literals_test;
