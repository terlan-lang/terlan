use std::env;
use std::fs;
use std::process::ExitCode;

use serde_json::json;
use terlan::compiler::syntax::{lex, parse_module_as_syntax_output, EbnfCompileError, TokenKind};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("terlan-parser-error-oracle: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<String>) -> Result<(), String> {
    let [input, output] = arguments.as_slice() else {
        return Err("usage: terlan-parser-error-oracle INPUT OUTPUT".to_owned());
    };
    let source =
        fs::read_to_string(input).map_err(|error| format!("cannot read {input}: {error}"))?;
    let artifact = match parse_module_as_syntax_output(&source) {
        Ok(_) => json!({
            "class": "none",
            "end": 0,
            "schema": "terlan.self-host.parser-error/v1",
            "start": 0,
            "valid": true,
        }),
        Err(EbnfCompileError::Parse(message, span)) => {
            let class = classify(&source, &message, span.start);
            let (start, end) = if matches!(
                class,
                "missing-module" | "invalid-module" | "source-before-module" | "lexical"
            ) {
                (span.start, span.end)
            } else {
                enclosing_declaration(&source, span.start).unwrap_or((span.start, span.end))
            };
            json!({
                "class": class,
                "end": end,
                "schema": "terlan.self-host.parser-error/v1",
                "start": start,
                "valid": false,
            })
        }
        Err(EbnfCompileError::Serialize(_)) => json!({
            "class": "serialize",
            "end": 0,
            "schema": "terlan.self-host.parser-error/v1",
            "start": 0,
            "valid": false,
        }),
    };
    let encoded = serde_json::to_vec_pretty(&artifact)
        .map_err(|error| format!("cannot encode parser error artifact: {error}"))?;
    fs::write(output, encoded).map_err(|error| format!("cannot write {output}: {error}"))
}

fn classify(source: &str, message: &str, offset: usize) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("unterminated") || lower.contains("end of input") {
        "unterminated-declaration"
    } else if lower.contains("lex") || lower.contains("character") {
        "lexical"
    } else {
        classify_source_region(source, offset)
    }
}

fn classify_source_region(source: &str, offset: usize) -> &'static str {
    let trimmed = source.trim_start();
    if !trimmed.starts_with("module ") {
        return if source
            .lines()
            .any(|line| line.trim_start().starts_with("module "))
        {
            "source-before-module"
        } else {
            "missing-module"
        };
    }

    let Some((start, end)) = enclosing_declaration(source, offset) else {
        return "expression";
    };
    let declaration = &source[start..end];
    let declaration_trimmed = declaration.trim_start();
    if declaration_trimmed.starts_with("module ") {
        return "invalid-module";
    }
    if declaration_trimmed.starts_with("type ")
        || declaration_trimmed.starts_with("pub type ")
        || declaration_trimmed.starts_with("opaque_type ")
        || declaration_trimmed.starts_with("pub opaque_type ")
    {
        return "type";
    }

    let relative_offset = offset.saturating_sub(start);
    let Some(arrow) = declaration.find("->") else {
        return "signature";
    };
    if relative_offset >= arrow {
        return "expression";
    }
    match (declaration.find('('), declaration[..arrow].rfind(')')) {
        (Some(open), Some(close)) if relative_offset > open && relative_offset <= close => {
            "pattern"
        }
        _ => "signature",
    }
}

fn enclosing_declaration(source: &str, offset: usize) -> Option<(usize, usize)> {
    let tokens = lex(source).ok()?;
    let mut depth = 0usize;
    let mut declaration_start = 0usize;
    for token in tokens {
        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                depth = depth.saturating_sub(1)
            }
            TokenKind::Dot if depth == 0 => {
                if offset <= token.end {
                    return Some((declaration_start, token.end));
                }
                declaration_start = token.end;
                while source
                    .as_bytes()
                    .get(declaration_start)
                    .is_some_and(u8::is_ascii_whitespace)
                {
                    declaration_start += 1;
                }
            }
            _ => {}
        }
    }
    Some((declaration_start, source.len()))
}
