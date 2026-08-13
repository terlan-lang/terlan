//! Explicit lowering from generated syntax nodes into the legacy compiler AST.
//!
//! LALRPOP owns recognition and span construction. This module deliberately
//! remains outside grammar actions so validation and compiler-model lowering
//! stay reviewable, testable phases.

mod binary_layout;
mod expressions;
mod module;
mod patterns;
mod raw_macros;

use super::{
    lalrpop_syntax::{LalrpopModuleSyntaxOutput, LalrpopSourceIndex, LalrpopSyntaxNode},
    parse_tree::{Expr, Module, Pattern, TypeExpr},
    span::Span,
};

/// Stable failure produced after generated parsing but before checked syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LalrpopLoweringError {
    pub(crate) message: String,
    pub(crate) span: Span,
}

pub(crate) type LalrpopLoweringResult<T> = Result<T, LalrpopLoweringError>;

pub(crate) fn lower_lalrpop_expression(
    source: &str,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    LalrpopLoweringContext::new(source).expression(node)
}

pub(crate) fn lower_lalrpop_pattern(
    source: &str,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Pattern> {
    LalrpopLoweringContext::new(source).pattern(node)
}

pub(crate) fn lower_lalrpop_module(
    source: &str,
    output: &LalrpopModuleSyntaxOutput,
) -> LalrpopLoweringResult<Module> {
    module::lower_module(source, output, false)
}

pub(crate) fn lower_lalrpop_interface_module(
    source: &str,
    output: &LalrpopModuleSyntaxOutput,
) -> LalrpopLoweringResult<Module> {
    module::lower_module(source, output, true)
}

pub(crate) struct LalrpopLoweringContext<'source> {
    source: &'source str,
    source_index: LalrpopSourceIndex,
}

impl<'source> LalrpopLoweringContext<'source> {
    pub(crate) fn new(source: &'source str) -> Self {
        Self {
            source,
            source_index: LalrpopSourceIndex::new(source),
        }
    }

    pub(crate) fn expression(&self, node: &LalrpopSyntaxNode) -> LalrpopLoweringResult<Expr> {
        expressions::lower_expression(self, node)
    }

    pub(crate) fn pattern(&self, node: &LalrpopSyntaxNode) -> LalrpopLoweringResult<Pattern> {
        patterns::lower_pattern(self, node)
    }

    pub(crate) fn type_expression(&self, node: &LalrpopSyntaxNode) -> TypeExpr {
        TypeExpr {
            text: canonical_type_text(self.text(node.span)),
            span: node.span,
        }
    }

    pub(crate) fn type_text(&self, node: &LalrpopSyntaxNode) -> String {
        canonical_type_text(self.text(node.span))
    }

    pub(crate) fn text(&self, span: Span) -> &'source str {
        self.source_index.text(self.source, span.start, span.end)
    }

    pub(crate) fn error(
        &self,
        node: &LalrpopSyntaxNode,
        message: impl Into<String>,
    ) -> LalrpopLoweringError {
        LalrpopLoweringError {
            message: message.into(),
            span: node.span,
        }
    }
}

fn canonical_type_text(source: &str) -> String {
    let chars = source.char_indices().collect::<Vec<_>>();
    let mut output = String::with_capacity(source.len());
    let mut index = 0usize;
    while index < chars.len() {
        let (offset, character) = chars[index];
        if character == ':' {
            let previous = output
                .chars()
                .rev()
                .find(|character| !character.is_whitespace());
            let next = chars.get(index + 1).copied();
            if matches!(
                previous,
                Some('{') | Some('[') | Some('(') | Some(',') | Some('|')
            ) && next.is_some_and(|(_, character)| character.is_ascii_lowercase())
            {
                let start = next.expect("checked atom start").0;
                let mut end_index = index + 1;
                while chars.get(end_index + 1).is_some_and(|(_, character)| {
                    character.is_ascii_alphanumeric() || *character == '_'
                }) {
                    end_index += 1;
                }
                let end = chars
                    .get(end_index + 1)
                    .map_or(source.len(), |(offset, _)| *offset);
                output.push_str("Atom[\"");
                output.push_str(&source[start..end]);
                output.push_str("\"]");
                index = end_index + 1;
                continue;
            }
        }
        let end = chars
            .get(index + 1)
            .map_or(source.len(), |(offset, _)| *offset);
        output.push_str(&source[offset..end]);
        index += 1;
    }
    normalize_type_spacing(&output)
}

fn normalize_type_spacing(source: &str) -> String {
    let characters = source.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(source.len() + 8);
    let mut index = 0usize;
    let mut in_string = false;
    while index < characters.len() {
        let character = characters[index];
        if character == '"' {
            in_string = !in_string;
            output.push(character);
            index += 1;
            continue;
        }
        if in_string {
            output.push(character);
            index += 1;
            continue;
        }
        if character.is_whitespace() {
            if !output.ends_with(' ')
                && !output.ends_with('[')
                && !output.ends_with('(')
                && !output.ends_with('{')
            {
                output.push(' ');
            }
            index += 1;
            continue;
        }
        if matches!(character, '=' | '-') && characters.get(index + 1) == Some(&'>') {
            trim_trailing_space(&mut output);
            output.push(' ');
            output.push(character);
            output.push('>');
            output.push(' ');
            index += 2;
            continue;
        }
        match character {
            ',' => {
                trim_trailing_space(&mut output);
                output.push_str(", ");
            }
            ':' => {
                trim_trailing_space(&mut output);
                output.push_str(": ");
            }
            '|' | '+' => {
                trim_trailing_space(&mut output);
                output.push(' ');
                output.push(character);
                output.push(' ');
            }
            ']' | ')' | '}' | '.' => {
                trim_trailing_space(&mut output);
                output.push(character);
            }
            '[' | '(' | '{' => {
                if character == '['
                    || output
                        .trim_end()
                        .chars()
                        .last()
                        .is_some_and(|previous| previous.is_alphanumeric() || previous == ']')
                {
                    trim_trailing_space(&mut output);
                }
                output.push(character);
            }
            _ => output.push(character),
        }
        index += 1;
    }
    output
        .trim()
        .replace(". ", ".")
        .replace("[ + _]", "[+_]")
        .replace("[ - _]", "[-_]")
}

fn trim_trailing_space(output: &mut String) {
    while output.ends_with(' ') {
        output.pop();
    }
}
