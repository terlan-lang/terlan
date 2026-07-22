use std::collections::HashSet;

use crate::terlan_html::escape_html_text;
use crate::terlan_purity::{syntax_declaration_callable_identity, CallableIdentity};
use crate::terlan_syntax::SyntaxDeclarationOutput;

/// Escapes untrusted documentation and signature text for generated HTML.
pub(super) fn sanitize_html_text(input: &str) -> String {
    escape_html_text(input)
}

/// Reports whether a function or method carries compiler-proven purity.
///
/// The proof set includes validated source assertions and body-inferred
/// callables, so documentation projects compiler metadata rather than merely
/// checking for a source-written annotation.
pub(super) fn declaration_is_compiler_pure(
    declaration: &SyntaxDeclarationOutput,
    known_pure: &HashSet<CallableIdentity>,
) -> bool {
    syntax_declaration_callable_identity(declaration)
        .is_some_and(|identity| known_pure.contains(&identity))
}
