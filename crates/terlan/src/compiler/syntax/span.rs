/// Byte-span location in Terlan source text.
///
/// Inputs:
/// - Start and end byte offsets in the original source string.
///
/// Output:
/// - Compact span value passed through parser, syntax output, diagnostics, and
///   downstream compiler phases.
///
/// Transformation:
/// - Keeps source location as byte offsets so each consumer can map to its own
///   display format without losing exact parser positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// Builds a source byte span.
    ///
    /// Inputs:
    /// - `start`: inclusive byte offset.
    /// - `end`: exclusive byte offset.
    ///
    /// Output:
    /// - `Span` containing the two offsets.
    ///
    /// Transformation:
    /// - Stores offsets without normalization so callers can preserve exact
    ///   parser state when constructing diagnostics.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}
