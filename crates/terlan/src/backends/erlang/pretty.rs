/// Returns Erlang source text in the current backend formatting mode.
///
/// Inputs:
/// - `input`: Erlang source text produced by backend rendering.
///
/// Output:
/// - Formatted Erlang source text.
///
/// Transformation:
/// - Currently preserves source text exactly while keeping a stable backend
///   hook for future Erlang pretty-printing.
pub fn pretty_print(input: &str) -> String {
    input.to_string()
}

#[cfg(test)]
#[path = "pretty_test.rs"]
mod pretty_test;
