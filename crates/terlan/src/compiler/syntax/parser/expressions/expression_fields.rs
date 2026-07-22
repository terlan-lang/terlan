/// Expression field grammar context for key class and separator validation.
///
/// Inputs: selected by the caller based on the production being parsed.
/// Output: passed to expression-field parsing as a compact policy value.
/// Transformation: distinguishes map expressions from Terlan records and
/// templates without changing the emitted field representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExprFieldKind {
    Map,
    TerlanRecord,
}
