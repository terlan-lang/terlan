use crate::terlan_syntax::{ebnf::EbnfSourceSpan, SyntaxPatternKind, SyntaxPatternOutput};

use super::{Analyzer, CoreBindingKind, LexicalEnvironment, Region};

impl Analyzer<'_> {
    pub(super) fn bind_pattern(
        &mut self,
        pattern: &SyntaxPatternOutput,
        span: EbnfSourceSpan,
        path: &str,
        environment: &mut LexicalEnvironment,
        region: &mut Region,
    ) {
        if let Some((name, kind)) = pattern_binding(pattern) {
            self.bind_name(&name, kind, span, path, environment, region);
        }
        for (index, child) in pattern.children.iter().enumerate() {
            self.bind_pattern(
                child,
                span,
                &format!("{path}:child:{index}"),
                environment,
                region,
            );
        }
        if pattern.kind == SyntaxPatternKind::BinaryLayout {
            // Binary-layout field keys are captures. Field values are
            // descriptor metadata represented as Var-shaped syntax nodes for
            // serialization compatibility, not source bindings.
            for (index, field) in pattern.fields.iter().enumerate() {
                self.bind_name(
                    &field.key,
                    CoreBindingKind::Pattern,
                    span,
                    &format!("{path}:field:{index}:{}", field.key),
                    environment,
                    region,
                );
            }
            return;
        }
        for (index, field) in pattern.fields.iter().enumerate() {
            self.bind_pattern(
                &field.value,
                span,
                &format!("{path}:field:{index}:{}", field.key),
                environment,
                region,
            );
        }
    }
}

fn pattern_binding(pattern: &SyntaxPatternOutput) -> Option<(String, CoreBindingKind)> {
    let text = pattern.text.as_deref()?;
    match pattern.kind {
        SyntaxPatternKind::Var => Some((text.to_string(), CoreBindingKind::Pattern)),
        SyntaxPatternKind::Alias => Some((text.to_string(), CoreBindingKind::Alias)),
        SyntaxPatternKind::StringCapture => Some((
            text.split(':').next().unwrap_or(text).trim().to_string(),
            CoreBindingKind::StringCapture,
        )),
        _ => None,
    }
}
