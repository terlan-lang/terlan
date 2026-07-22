/// Expression-field grammar context for key and separator validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExprFieldKind {
    Map,
    TerlanRecord,
}
