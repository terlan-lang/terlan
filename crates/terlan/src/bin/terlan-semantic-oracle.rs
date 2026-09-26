use std::env;
use std::fmt::Write as _;
use std::fs;
use std::process::ExitCode;

use terlan::compiler::syntax::{parse_module_as_syntax_output, SyntaxDeclarationPayload};
use terlan::compiler::typeck::analyze_syntax_bindings;

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(input) = arguments.next() else {
        eprintln!("usage: terlan-semantic-oracle <input> <output>");
        return ExitCode::FAILURE;
    };
    let Some(output) = arguments.next() else {
        eprintln!("missing output path");
        return ExitCode::FAILURE;
    };
    let source = match fs::read_to_string(&input) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{input}: {error:?}");
            return ExitCode::FAILURE;
        }
    };
    let module = match parse_module_as_syntax_output(&source) {
        Ok(module) => module,
        Err(error) => {
            eprintln!("{input}: {error:?}");
            return ExitCode::FAILURE;
        }
    };
    let analysis = analyze_syntax_bindings(&module);
    let mut rendered = String::from("schema terlan.self-host.semantic-common/v1\n");

    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                return_type,
                ..
            }
            | SyntaxDeclarationPayload::ConstFunction {
                name,
                params,
                return_type,
                ..
            } => {
                let _ = writeln!(
                    rendered,
                    "callable\t{name}\t{}\t{}\t{}",
                    params.len(),
                    return_type.text,
                    declaration.span.start
                );
            }
            SyntaxDeclarationPayload::Method {
                name,
                params,
                return_type,
                ..
            } => {
                let _ = writeln!(
                    rendered,
                    "callable\t{name}\t{}\t{}\t{}",
                    params.len() + 1,
                    return_type.text,
                    declaration.span.start
                );
            }
            _ => {}
        }
    }
    for binding in &analysis.evidence.bindings {
        let _ = writeln!(
            rendered,
            "declaration\t{}\t{}",
            binding.name, binding.span_start
        );
    }
    for reference in &analysis.evidence.references {
        let declaration_offset = analysis
            .evidence
            .bindings
            .iter()
            .find(|binding| binding.id == reference.binding)
            .map_or(usize::MAX, |binding| binding.span_start);
        let normalized = if declaration_offset == usize::MAX {
            -1
        } else {
            declaration_offset as isize
        };
        let _ = writeln!(
            rendered,
            "reference\t{}\t{}\t{}",
            reference.name, reference.span_start, normalized
        );
    }
    for collision in &analysis.collisions {
        let _ = writeln!(rendered, "diagnostic\t{}", collision.span.start);
    }
    if let Err(error) = fs::write(&output, rendered) {
        eprintln!("{output}: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
