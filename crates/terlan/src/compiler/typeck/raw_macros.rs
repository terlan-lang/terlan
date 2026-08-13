#[path = "raw_macros/diagnostics.rs"]
mod diagnostics;
#[path = "raw_macros/expansion.rs"]
mod expansion;

pub use diagnostics::collect_syntax_raw_macro_diagnostics;
pub(crate) use diagnostics::{
    raw_macro_requires_resolution_diagnostic, raw_macro_resolution_message_for_expr,
};
pub use expansion::{
    collect_syntax_unsupported_raw_declaration_diagnostics, expand_syntax_macros_with_interfaces,
    expand_syntax_raw_macros,
};
