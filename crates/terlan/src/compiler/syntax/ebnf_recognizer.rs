//! Test-only EBNF membership oracle. Does not use the Terlan lexer or parser.
//!
//! Earley recognition handles ambiguity, left recursion and nullable repetition.
//! Lexical roots are matched atomically against expressions derived from the EBNF.
//! Unknown predicates and resource exhaustion are errors, never rejections.

use std::collections::{BTreeMap, HashSet};

use regex::Regex;

use super::ebnf::{EbnfGrammarContract, EbnfGrammarExpr, EbnfGrammarExprKind as Kind};

const LEXICAL_ROOTS: &[&str] = &[
    "LowerIdent",
    "UpperIdent",
    "Binding",
    "Int",
    "Float",
    "StringLiteral",
];
// Global keywords from the language's lexical policy. Contextual words (as, mut,
// annotation, etc.) remain identifiers. Kept independent of compiler token kinds.
const RESERVED: &[&str] = &[
    "module",
    "pub",
    "const",
    "macro",
    "constructor",
    "export",
    "import",
    "type",
    "nominal",
    "opaque",
    "trait",
    "impl",
    "implements",
    "includes",
    "for",
    "template",
    "where",
    "extends",
    "struct",
    "case",
    "try",
    "catch",
    "after",
    "let",
    "if",
    "when",
    "with",
    "and",
    "in",
    "or",
    "div",
    "rem",
];
const EOF_PREDICATE: &str = "end of input after optional whitespace and comments";
const STRING_PREDICATE: &str = "any source character except unescaped terminator";

#[derive(Clone)]
enum Symbol {
    Rule(usize),
    Literal(String),
    Lexical(String, Regex),
    End,
    Unsupported(String),
}

struct Production {
    lhs: usize,
    rhs: Vec<Symbol>,
}

pub(super) struct Recognizer {
    names: BTreeMap<String, usize>,
    productions: Vec<Production>,
    by_rule: Vec<Vec<usize>>,
    punctuation: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Item {
    production: usize,
    dot: usize,
    origin: usize,
}

#[derive(Default)]
struct Column {
    items: Vec<Item>,
    seen: HashSet<Item>,
}

impl Column {
    fn insert(&mut self, item: Item, remaining: &mut usize) -> Result<(), String> {
        if self.seen.insert(item) {
            if *remaining == 0 {
                return Err("EBNF recognition exceeded its state budget".into());
            }
            *remaining -= 1;
            self.items.push(item);
        }
        Ok(())
    }
}

impl Recognizer {
    pub(super) fn new(grammar: &EbnfGrammarContract) -> Result<Self, String> {
        let mut result = Self {
            names: BTreeMap::new(),
            productions: Vec::new(),
            by_rule: vec![Vec::new(); grammar.rules.len()],
            punctuation: Vec::new(),
        };
        for (id, rule) in grammar.rules.iter().enumerate() {
            if result.names.insert(rule.name.clone(), id).is_some() {
                return Err(format!("duplicate EBNF rule {}", rule.name));
            }
        }
        for rule in &grammar.rules {
            let lhs = result.names[&rule.name];
            let symbol = if LEXICAL_ROOTS.contains(&rule.name.as_str()) {
                let pattern = lexical_pattern(grammar, &rule.expr, &mut vec![rule.name.clone()])?;
                Symbol::Lexical(
                    rule.name.clone(),
                    Regex::new(&format!("^(?:{pattern})$")).map_err(|e| e.to_string())?,
                )
            } else {
                result.lower(&rule.expr)?
            };
            result.add(lhs, vec![symbol]);
        }
        Ok(result)
    }

    fn add(&mut self, lhs: usize, rhs: Vec<Symbol>) {
        self.by_rule[lhs].push(self.productions.len());
        self.productions.push(Production { lhs, rhs });
    }

    fn lower(&mut self, expr: &EbnfGrammarExpr) -> Result<Symbol, String> {
        match &expr.kind {
            Kind::Nonterminal { name } => {
                return self
                    .names
                    .get(name)
                    .copied()
                    .map(Symbol::Rule)
                    .ok_or_else(|| format!("undefined EBNF rule {name}"))
            }
            Kind::Terminal { value } => {
                if value.len() > 1
                    && value
                        .chars()
                        .all(|c| !c.is_alphanumeric() && !c.is_whitespace())
                {
                    self.punctuation.push(value.clone());
                }
                return Ok(Symbol::Literal(value.clone()));
            }
            Kind::Special { text } if text.trim() == EOF_PREDICATE => return Ok(Symbol::End),
            Kind::Special { text } => return Ok(Symbol::Unsupported(text.clone())),
            Kind::CharacterClass { chars } => {
                return Ok(Symbol::Unsupported(format!("character class {chars}")))
            }
            _ => {}
        }
        let lhs = self.by_rule.len();
        self.by_rule.push(Vec::new());
        match &expr.kind {
            Kind::Sequence { items } => {
                let rhs = items
                    .iter()
                    .map(|item| self.lower(item))
                    .collect::<Result<_, _>>()?;
                self.add(lhs, rhs);
            }
            Kind::Alternation { items } => {
                for item in items {
                    let symbol = self.lower(item)?;
                    self.add(lhs, vec![symbol]);
                }
            }
            Kind::Group { expr } => {
                let symbol = self.lower(expr)?;
                self.add(lhs, vec![symbol]);
            }
            Kind::Optional { expr: inner }
            | Kind::Repetition { expr: inner }
            | Kind::OneOrMore { expr: inner } => {
                let symbol = self.lower(inner)?;
                if !matches!(&expr.kind, Kind::OneOrMore { .. }) {
                    self.add(lhs, vec![]);
                }
                self.add(lhs, vec![symbol.clone()]);
                if !matches!(&expr.kind, Kind::Optional { .. }) {
                    self.add(lhs, vec![symbol, Symbol::Rule(lhs)]);
                }
            }
            _ => unreachable!(),
        }
        Ok(Symbol::Rule(lhs))
    }

    pub(super) fn recognizes(&self, entry: &str, source: &str) -> Result<bool, String> {
        self.recognizes_with_budget(entry, source, 250_000)
    }

    fn recognizes_with_budget(
        &self,
        entry: &str,
        source: &str,
        mut budget: usize,
    ) -> Result<bool, String> {
        if source.len() > 16_384 {
            return Err("EBNF reference input exceeds 16 KiB".into());
        }
        let root = *self
            .names
            .get(entry)
            .ok_or_else(|| format!("unknown entry rule {entry}"))?;
        let mut chart: Vec<Column> = (0..=source.len()).map(|_| Column::default()).collect();
        for &production in &self.by_rule[root] {
            chart[0].insert(
                Item {
                    production,
                    dot: 0,
                    origin: 0,
                },
                &mut budget,
            )?;
        }
        let mut unsupported = None;
        for position in 0..=source.len() {
            let mut cursor = 0;
            while cursor < chart[position].items.len() {
                let item = chart[position].items[cursor];
                cursor += 1;
                let production = &self.productions[item.production];
                match production.rhs.get(item.dot) {
                    None => {
                        if production.lhs == root
                            && item.origin == 0
                            && skip_trivia(source, position)? == source.len()
                        {
                            return Ok(true);
                        }
                        let waiting = chart[item.origin].items.clone();
                        for previous in waiting {
                            if matches!(self.productions[previous.production].rhs.get(previous.dot), Some(Symbol::Rule(rule)) if *rule == production.lhs)
                            {
                                chart[position].insert(
                                    Item {
                                        dot: previous.dot + 1,
                                        ..previous
                                    },
                                    &mut budget,
                                )?;
                            }
                        }
                    }
                    Some(Symbol::Rule(rule)) => {
                        for &production in &self.by_rule[*rule] {
                            chart[position].insert(
                                Item {
                                    production,
                                    dot: 0,
                                    origin: position,
                                },
                                &mut budget,
                            )?;
                        }
                        // A nullable completion may have been processed before this
                        // waiting item was inserted in the same column.
                        let nullable = chart[position].items.iter().any(|done| {
                            let candidate = &self.productions[done.production];
                            done.origin == position
                                && candidate.lhs == *rule
                                && done.dot == candidate.rhs.len()
                        });
                        if nullable {
                            chart[position].insert(
                                Item {
                                    dot: item.dot + 1,
                                    ..item
                                },
                                &mut budget,
                            )?;
                        }
                    }
                    Some(Symbol::Unsupported(description)) => {
                        unsupported = Some(description.clone())
                    }
                    Some(symbol) => {
                        if let Some(end) = self.scan(symbol, source, position)? {
                            chart[end].insert(
                                Item {
                                    dot: item.dot + 1,
                                    ..item
                                },
                                &mut budget,
                            )?;
                        }
                    }
                }
            }
        }
        match unsupported {
            Some(description) => Err(format!(
                "recognition requires unsupported EBNF predicate: {description}"
            )),
            None => Ok(false),
        }
    }

    fn scan(
        &self,
        symbol: &Symbol,
        source: &str,
        position: usize,
    ) -> Result<Option<usize>, String> {
        let start = skip_trivia(source, position)?;
        let rest = &source[start..];
        let length = match symbol {
            Symbol::End => return Ok((start == source.len()).then_some(start)),
            Symbol::Literal(value) if rest.starts_with(value) => {
                let end = value.len();
                if value.chars().last().is_some_and(ident_continue)
                    && rest[end..].chars().next().is_some_and(ident_continue)
                {
                    return Ok(None);
                }
                if self.punctuation.iter().any(|longer| {
                    longer.len() > end
                        && longer.starts_with(value.as_str())
                        && rest.starts_with(longer)
                }) {
                    return Ok(None);
                }
                end
            }
            Symbol::Lexical(name, pattern) => {
                let end = lexeme_end(rest);
                let value = &rest[..end];
                if end == 0
                    || !pattern.is_match(value)
                    || (matches!(name.as_str(), "LowerIdent" | "Binding")
                        && RESERVED.contains(&value))
                {
                    return Ok(None);
                }
                end
            }
            _ => return Ok(None),
        };
        Ok(Some(start + length))
    }
}

fn ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn lexeme_end(source: &str) -> usize {
    let bytes = source.as_bytes();
    let Some(&first) = bytes.first() else {
        return 0;
    };
    if first == b'"' {
        let mut escaped = false;
        for (offset, c) in source[1..].char_indices() {
            if !escaped && c == '"' {
                return offset + 2;
            }
            escaped = !escaped && c == '\\';
        }
        return 0;
    }
    if first.is_ascii_alphabetic() || first == b'_' {
        return bytes
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric() || **c == b'_')
            .count();
    }
    if first.is_ascii_digit() {
        let mut end = 1;
        while end < bytes.len() {
            let c = bytes[end];
            if c.is_ascii_alphanumeric()
                || c == b'_'
                || (c == b'.' && bytes.get(end + 1).is_some_and(u8::is_ascii_digit))
                || (matches!(c, b'+' | b'-')
                    && matches!(bytes[end - 1], b'e' | b'E')
                    && !source.starts_with("0x"))
            {
                end += 1;
            } else {
                break;
            }
        }
        return end;
    }
    0
}

fn skip_trivia(source: &str, mut position: usize) -> Result<usize, String> {
    loop {
        let rest = &source[position..];
        let trimmed = rest.trim_start_matches(char::is_whitespace);
        position += rest.len() - trimmed.len();
        if trimmed.starts_with("//") {
            position += trimmed.find('\n').unwrap_or(trimmed.len());
        } else if let Some(comment) = trimmed.strip_prefix("/*") {
            let end = comment
                .find("*/")
                .ok_or("unterminated block comment in EBNF input")?;
            position += end + 4;
        } else {
            return Ok(position);
        }
    }
}

fn lexical_pattern(
    grammar: &EbnfGrammarContract,
    expr: &EbnfGrammarExpr,
    stack: &mut Vec<String>,
) -> Result<String, String> {
    Ok(match &expr.kind {
        Kind::Terminal { value } => regex::escape(value),
        Kind::Nonterminal { name } => {
            if stack.contains(name) {
                return Err(format!("recursive lexical rule {name}"));
            }
            let rule = grammar
                .rule(name)
                .ok_or_else(|| format!("undefined lexical rule {name}"))?;
            stack.push(name.clone());
            let pattern = lexical_pattern(grammar, &rule.expr, stack)?;
            stack.pop();
            pattern
        }
        Kind::Sequence { items } | Kind::Alternation { items } => {
            let parts = items
                .iter()
                .map(|item| lexical_pattern(grammar, item, stack))
                .collect::<Result<Vec<_>, _>>()?;
            format!(
                "(?:{})",
                parts.join(if matches!(&expr.kind, Kind::Alternation { .. }) {
                    "|"
                } else {
                    ""
                })
            )
        }
        Kind::Optional { expr: inner }
        | Kind::Repetition { expr: inner }
        | Kind::OneOrMore { expr: inner }
        | Kind::Group { expr: inner } => {
            let suffix = match &expr.kind {
                Kind::Optional { .. } => "?",
                Kind::Repetition { .. } => "*",
                Kind::OneOrMore { .. } => "+",
                _ => "",
            };
            format!("(?:{}){suffix}", lexical_pattern(grammar, inner, stack)?)
        }
        Kind::Special { text } if text.trim() == STRING_PREDICATE => {
            r#"(?:\\[\s\S]|[^"\\])"#.into()
        }
        _ => return Err(format!("unsupported lexical expression {:?}", expr.kind)),
    })
}

#[cfg(test)]
#[path = "ebnf_recognizer_test.rs"]
mod tests;
