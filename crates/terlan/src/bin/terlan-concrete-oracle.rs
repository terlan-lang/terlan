use std::env;
use std::fmt::Write;
use std::fs;
use std::process::ExitCode;

use terlan::compiler::syntax::{
    lalrpop_boundary::parse_lalrpop_token_output, parse_module_as_syntax_output,
};

#[derive(Debug)]
struct Node {
    terminal: String,
    start: usize,
    end: usize,
    lexeme_bytes: usize,
    children: Vec<Node>,
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("terlan-concrete-oracle: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let [input, output] = arguments.as_slice() else {
        return Err(("usage: terlan-concrete-oracle INPUT OUTPUT".to_owned()).into());
    };
    let source =
        fs::read_to_string(input).map_err(|error| format!("cannot read {input}: {error}"))?;
    parse_module_as_syntax_output(&source)
        .map_err(|error| format!("canonical parser rejected {input}: {error:?}"))?;
    let projected = parse_lalrpop_token_output(&source)
        .map_err(|error| format!("cannot project canonical tokens for {input}: {error:?}"))?;
    let leaves = projected
        .tokens
        .into_iter()
        .map(|token| {
            let start = token.span.start;
            let end = token.span.end;
            let text = source
                .get(start..end)
                .ok_or_else(|| format!("projected token span {start}..{end} is outside source"))?;
            Ok((token.terminal.to_owned(), start, end, text.to_owned()))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let (nodes, consumed) = parse_nodes(&leaves, 0, None)?;
    if consumed != leaves.len() {
        return Err(("concrete parser left projected tokens unconsumed".to_owned()).into());
    }
    let mut rendered = String::from(
        "schema terlan.self-host.concrete-syntax/v1\nvalid\ttrue\nmessage_bytes\t0\ndepth\tterminal\tstart\tend\tlexeme_bytes\n",
    );
    render_nodes(&nodes, 0, &mut rendered)
        .map_err(|_| "cannot render concrete syntax evidence".to_owned())?;
    Ok(fs::write(output, rendered).map_err(|error| format!("cannot write {output}: {error}"))?)
}

fn parse_nodes(
    tokens: &[(String, usize, usize, String)],
    mut index: usize,
    expected_close: Option<&str>,
) -> Result<(Vec<Node>, usize), Box<dyn std::error::Error>> {
    let mut nodes = Vec::new();
    while let Some((terminal, start, end, text)) = tokens.get(index) {
        if expected_close == Some(text.as_str()) {
            return Ok((nodes, index + 1));
        }
        if matches!(text.as_str(), ")" | "]" | "}") {
            return Err((format!("unexpected closing delimiter {text:?} at {start}")).into());
        }
        let close = match text.as_str() {
            "(" => Some((")", "group-()")),
            "[" => Some(("]", "group-[]")),
            "{" => Some(("}", "group-{}")),
            _ => None,
        };
        if let Some((close_text, group_terminal)) = close {
            let (children, next) = parse_nodes(tokens, index + 1, Some(close_text))?;
            let group_end = tokens
                .get(next.saturating_sub(1))
                .map(|token| token.2)
                .ok_or_else(|| format!("missing closing delimiter {close_text:?}"))?;
            nodes.push(Node {
                terminal: group_terminal.to_owned(),
                start: *start,
                end: group_end,
                lexeme_bytes: 0,
                children,
            });
            index = next;
        } else {
            nodes.push(Node {
                terminal: terminal.clone(),
                start: *start,
                end: *end,
                lexeme_bytes: text.len(),
                children: Vec::new(),
            });
            index += 1;
        }
    }
    if let Some(close) = expected_close {
        Err((format!("missing closing delimiter {close:?}")).into())
    } else {
        Ok((nodes, index))
    }
}

fn render_nodes(nodes: &[Node], depth: usize, output: &mut String) -> std::fmt::Result {
    for node in nodes {
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}",
            depth, node.terminal, node.start, node.end, node.lexeme_bytes
        )?;
        render_nodes(&node.children, depth + 1, output)?;
    }
    Ok(())
}
