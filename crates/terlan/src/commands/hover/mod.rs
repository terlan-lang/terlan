use std::collections::HashMap;
use std::process::ExitCode;

use crate::terlan_hir::{
    load_interfaces_from_file_set, resolve_syntax_module_output_with_interfaces, ModuleInterface,
};
use crate::terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output, EbnfCompileError,
    SyntaxDeclarationPayload, SyntaxModuleOutput,
};
use crate::terlan_typeck::type_check_syntax_module_output;

use crate::{CliCommand, CliState};

mod html;
pub(crate) use html::hover_component_prop_type;
mod scanner;
pub(crate) use scanner::{ident_span_at_offset, read_ident_at, record_access_at};

use scanner::is_identifier_byte;

/// Executes the `hover` CLI command.
///
/// Inputs:
/// - `cmd`: parsed CLI command containing `hover` command-local arguments.
/// - `state`: parsed global CLI state, including diagnostic output format.
///
/// Output:
/// - `ExitCode::SUCCESS` when hover type or documentation text is printed.
/// - `ExitCode::from(2)` for malformed arguments or out-of-range positions.
/// - `ExitCode::from(1)` for read, parse, typecheck, or missing-hover failures.
///
/// Transformation:
/// - Parses file/line/column arguments, validates the source through parse and
///   typecheck, converts the source position into a byte offset, and prints the
///   best available hover result.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    let (path, line, column) = match parse_hover_args(&cmd.args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{}", message);
            crate::print_usage();
            return ExitCode::from(2);
        }
    };

    let source = match crate::support::read_file(path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };

    let module = match parse_source(path, &source) {
        Ok(module) => module,
        Err((message, span)) => {
            crate::support::emit_diagnostic(
                "parse_error",
                &message,
                path,
                span.start,
                span.end,
                state.diagnostic_format,
            );
            return ExitCode::from(1);
        }
    };

    let interfaces = load_interfaces_from_file_set(path);
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    let mut has_errors = false;
    for diag in diagnostics {
        let is_warning = matches!(diag.severity, crate::terlan_typeck::DiagSeverity::Warning);
        has_errors = has_errors || !is_warning;
        if !is_warning {
            crate::support::emit_diagnostic(
                "type_error",
                &diag.message,
                path,
                diag.span.start,
                diag.span.end,
                state.diagnostic_format,
            );
        }
    }
    if has_errors {
        return ExitCode::from(1);
    }

    let offset = match line_column_to_offset(&source, line, column) {
        Some(offset) => offset,
        None => {
            eprintln!("hover position is outside the source");
            return ExitCode::from(2);
        }
    };

    match hover_component_prop_type(&module, &source, offset)
        .or_else(|| hover_record_field_type(&module, &source, offset))
    {
        Some(ty) => {
            println!("{}", ty);
            ExitCode::SUCCESS
        }
        None => {
            if let Some(docs) = hover_local_docs(&module, &source, offset) {
                println!("{}", docs);
                ExitCode::SUCCESS
            } else if let Some(docs) = hover_imported_docs(&module, &interfaces, &source, offset) {
                println!("{}", docs);
                ExitCode::SUCCESS
            } else {
                eprintln!("no hover information available");
                ExitCode::from(1)
            }
        }
    }
}

/// Parsed hover source or parse diagnostic span.
///
/// Inputs:
/// - Source text parsed by the hover command.
///
/// Output:
/// - `Ok(SyntaxModuleOutput)` for a parsed module.
/// - `Err((message, range))` for parse failures that should be reported at the
///   source span.
///
/// Transformation:
/// - Keeps hover parsing errors in the small shape required by diagnostic
///   emission instead of exposing parser internals to the command flow.
type HoverParseResult = Result<SyntaxModuleOutput, (String, std::ops::Range<usize>)>;

/// Parses a hover source file through the formal syntax-output parser pipeline.
///
/// Inputs:
/// - `path`: command input path used to choose module or interface parsing.
/// - `source`: raw Terlan source text.
///
/// Output:
/// - Parsed syntax-output module or parser error.
///
/// Transformation:
/// - Dispatches `.terli` files to interface parsing and other files to source
///   module parsing.
fn parse_source(path: &str, source: &str) -> HoverParseResult {
    if path.ends_with(".terli") {
        parse_interface_module_as_syntax_output(source)
    } else {
        parse_module_as_syntax_output(source)
    }
    .map_err(|error| match error {
        EbnfCompileError::Parse(message, span) => (message, span.start..span.end),
        EbnfCompileError::Serialize(message) => (message, 0..0),
    })
}

/// Parses command-local arguments for `hover`.
///
/// Inputs:
/// - `args`: arguments after the `hover` verb.
///
/// Output:
/// - Source path plus one-based line and column.
/// - `Err(String)` for missing, unexpected, or non-positive coordinates.
///
/// Transformation:
/// - Scans `--line` and `--column`/`--col` flags while preserving the borrowed
///   source path.
pub(crate) fn parse_hover_args(args: &[String]) -> Result<(&str, usize, usize), String> {
    if args.len() != 5 {
        return Err("hover requires a path, --line, and --column".to_string());
    }

    let path = args[0].as_str();
    let mut line = None;
    let mut column = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--line" if i + 1 < args.len() => {
                line = args[i + 1].parse::<usize>().ok();
                i += 2;
            }
            "--column" | "--col" if i + 1 < args.len() => {
                column = args[i + 1].parse::<usize>().ok();
                i += 2;
            }
            other => {
                return Err(format!("unexpected hover argument: {}", other));
            }
        }
    }

    match (line, column) {
        (Some(line), Some(column)) if line > 0 && column > 0 => Ok((path, line, column)),
        _ => Err("hover line and column must be positive integers".to_string()),
    }
}

/// Converts a one-based line/column position into a byte offset.
///
/// Inputs:
/// - `source`: source text.
/// - `line`: one-based line number.
/// - `column`: one-based column number.
///
/// Output:
/// - Byte offset for the position, including EOF position, or `None`.
///
/// Transformation:
/// - Walks Unicode scalar boundaries and counts newlines/columns.
pub(crate) fn line_column_to_offset(source: &str, line: usize, column: usize) -> Option<usize> {
    let mut current_line = 1usize;
    let mut current_column = 1usize;

    for (offset, ch) in source.char_indices() {
        if current_line == line && current_column == column {
            return Some(offset);
        }

        if ch == '\n' {
            current_line += 1;
            current_column = 1;
        } else {
            current_column += 1;
        }
    }

    if current_line == line && current_column == column {
        Some(source.len())
    } else {
        None
    }
}

/// Returns the type of a record field under a hover position.
///
/// Inputs:
/// - `module`: parsed module containing struct declarations.
/// - `source`: source text containing the hover position.
/// - `offset`: hover byte offset.
///
/// Output:
/// - Field annotation text when the offset is on a known record field.
///
/// Transformation:
/// - Detects `Record#field` access syntax and resolves the field against struct
///   declarations.
pub(crate) fn hover_record_field_type(
    module: &SyntaxModuleOutput,
    source: &str,
    offset: usize,
) -> Option<String> {
    let (struct_name, field_name) = record_access_at(source, offset)?;
    module.declarations.iter().find_map(|decl| {
        let SyntaxDeclarationPayload::Struct { name, fields, .. } = &decl.payload else {
            return None;
        };
        if name != &struct_name {
            return None;
        }
        fields
            .iter()
            .find(|field| field.name == field_name)
            .map(|field| field.annotation.text.clone())
    })
}

/// Returns local documentation under a hover position.
///
/// Inputs:
/// - `module`: parsed module.
/// - `source`: source text.
/// - `offset`: hover byte offset.
///
/// Output:
/// - Joined documentation text for local declarations/fields, or `None`.
///
/// Transformation:
/// - Resolves record-field, type, struct, function, and trait hover targets
///   against local declarations.
pub(crate) fn hover_local_docs(
    module: &SyntaxModuleOutput,
    source: &str,
    offset: usize,
) -> Option<String> {
    if let Some((struct_name, field_name)) = record_access_at(source, offset) {
        if let Some(docs) = module.declarations.iter().find_map(|decl| {
            let SyntaxDeclarationPayload::Struct { name, fields, .. } = &decl.payload else {
                return None;
            };
            if name != &struct_name {
                return None;
            }
            fields
                .iter()
                .find(|field| field.name == field_name && !field.docs.is_empty())
                .map(|field| field.docs.join("\n"))
        }) {
            return Some(docs);
        }
    }

    let ident = ident_at_offset(source, offset)?;
    module
        .declarations
        .iter()
        .find_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Type { name, .. }
                if name == &ident && !decl.docs.is_empty() =>
            {
                Some(decl.docs.join("\n"))
            }
            SyntaxDeclarationPayload::Struct { name, fields, .. }
                if name == &ident && !decl.docs.is_empty() =>
            {
                Some(decl.docs.join("\n")).or_else(|| {
                    fields
                        .iter()
                        .find(|field| field.name == ident && !field.docs.is_empty())
                        .map(|field| field.docs.join("\n"))
                })
            }
            SyntaxDeclarationPayload::Struct { fields, .. } => fields
                .iter()
                .find(|field| field.name == ident && !field.docs.is_empty())
                .map(|field| field.docs.join("\n")),
            SyntaxDeclarationPayload::Function { name, .. }
                if name == &ident && !decl.docs.is_empty() =>
            {
                Some(decl.docs.join("\n"))
            }
            SyntaxDeclarationPayload::Trait {
                name,
                params,
                is_public,
                ..
            } if name == &ident
                || ((ident == "trait" || ident == "pub")
                    && decl.span.start <= offset
                    && offset <= decl.span.end) =>
            {
                Some(hover_trait_summary(name, params, *is_public, &decl.docs))
            }
            _ => None,
        })
}

/// Builds a compact trait summary for hover output.
///
/// Inputs:
/// - `trait_name`: trait identifier.
///
/// Output:
/// - Documentation plus signature summary.
///
/// Transformation:
/// - Joins docs and formats visibility, name, and type parameters.
fn hover_trait_summary(
    trait_name: &str,
    trait_params: &[String],
    is_public: bool,
    docs: &[String],
) -> String {
    let mut out = String::new();
    if !docs.is_empty() {
        out.push_str(&docs.join("\n"));
        out.push_str("\n\n");
    }
    out.push_str(if is_public { "pub trait " } else { "trait " });
    out.push_str(trait_name);
    if !trait_params.is_empty() {
        out.push('[');
        out.push_str(&trait_params.join(", "));
        out.push(']');
    }
    out
}

/// Returns imported documentation under a hover position.
///
/// Inputs:
/// - `module`: parsed module containing imports.
/// - `interfaces`: loaded module interfaces keyed by module name.
/// - `source`: source text.
/// - `offset`: hover byte offset.
///
/// Output:
/// - Imported item documentation text, or `None`.
///
/// Transformation:
/// - Resolves aliased imports and qualified import member hovers against
///   interface documentation.
pub(crate) fn hover_imported_docs(
    module: &SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
    source: &str,
    offset: usize,
) -> Option<String> {
    let ident = ident_at_offset(source, offset)?;
    for decl in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            module_name, items, ..
        } = &decl.payload
        else {
            continue;
        };
        let Some(interface) = interfaces.get(module_name) else {
            continue;
        };
        for item in items {
            let local_name = item.as_alias.as_ref().unwrap_or(&item.name);
            if local_name == &ident {
                return interface_item_docs(interface, &item.name);
            }
        }
    }

    let Some((module_name, member_name)) = qualified_import_member(source, offset) else {
        return None;
    };
    for decl in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            module_name: import_module_name,
            items,
            ..
        } = &decl.payload
        else {
            continue;
        };
        if import_module_name != &module_name {
            continue;
        }
        let Some(interface) = interfaces.get(import_module_name) else {
            continue;
        };
        for item in items {
            if item.name == member_name {
                return interface_item_docs(interface, &item.name);
            }
        }
    }
    None
}

/// Returns documentation for an item in an interface.
///
/// Inputs:
/// - `interface`: imported module interface.
/// - `name`: item name to look up.
///
/// Output:
/// - Joined documentation text, or `None`.
///
/// Transformation:
/// - Checks type docs, public function docs by arity order, and trait docs.
pub(crate) fn interface_item_docs(interface: &ModuleInterface, name: &str) -> Option<String> {
    if let Some(docs) = interface
        .type_docs
        .get(name)
        .filter(|docs| !docs.is_empty())
    {
        return Some(docs.join("\n"));
    }

    let mut functions: Vec<_> = interface
        .functions
        .iter()
        .filter(|((function_name, _arity), signature)| function_name == name && signature.public)
        .collect();
    functions.sort_by_key(|(key, _)| key.1);
    if let Some(docs) = functions
        .into_iter()
        .find_map(|(_, signature)| (!signature.docs.is_empty()).then(|| signature.docs.join("\n")))
    {
        return Some(docs);
    }

    if let Some(trait_signature) = interface.traits.get(name) {
        (!trait_signature.docs.is_empty()).then(|| trait_signature.docs.join("\n"))
    } else {
        None
    }
}

/// Returns the identifier under a byte offset.
///
/// Inputs:
/// - `source`: source text.
/// - `offset`: byte offset.
///
/// Output:
/// - Identifier text or `None`.
///
/// Transformation:
/// - Uses `ident_span_at_offset` and slices the source span.
pub(crate) fn ident_at_offset(source: &str, offset: usize) -> Option<String> {
    let (start, end) = ident_span_at_offset(source, offset)?;
    Some(source[start..end].to_string())
}

/// Returns a qualified import member under a hover position.
///
/// Inputs:
/// - `source`: source text.
/// - `offset`: byte offset on the member name.
///
/// Output:
/// - Qualified module prefix and member name, or `None`.
///
/// Transformation:
/// - Requires the member to be preceded by `.` and scans module-name segments
///   before that dot.
pub(crate) fn qualified_import_member(source: &str, offset: usize) -> Option<(String, String)> {
    let (member_start, member_end) = ident_span_at_offset(source, offset)?;
    if member_start == 0 || source.as_bytes()[member_start - 1] != b'.' {
        return None;
    }

    let module_name = qualified_module_prefix_before(source, member_start - 1)?;
    if module_name.is_empty() {
        return None;
    }

    let member_name = source[member_start..member_end].to_string();
    Some((module_name, member_name))
}

/// Reads a dotted module prefix before a dot.
///
/// Inputs:
/// - `source`: source text.
/// - `offset`: byte offset expected to contain `.`.
///
/// Output:
/// - Dotted module prefix or `None`.
///
/// Transformation:
/// - Scans identifier segments backward, reverses them, and joins with dots.
fn qualified_module_prefix_before(source: &str, offset: usize) -> Option<String> {
    let bytes = source.as_bytes();
    if offset == 0 || bytes[offset] != b'.' {
        return None;
    }

    let mut cursor = offset;
    let mut segments = Vec::new();
    loop {
        if cursor == 0 {
            break;
        }
        let segment_end = cursor;
        let mut segment_start = segment_end;
        while segment_start > 0 && is_identifier_byte(bytes[segment_start - 1]) {
            segment_start -= 1;
        }

        if segment_start == segment_end {
            break;
        }

        segments.push(&source[segment_start..segment_end]);

        if segment_start == 0 {
            break;
        }
        if segment_start == cursor {
            return None;
        }
        if bytes[segment_start - 1] != b'.' {
            break;
        }
        cursor = segment_start - 1;
    }

    segments.reverse();
    Some(segments.join("."))
}

#[cfg(test)]
#[path = "hover_test.rs"]
mod hover_test;
