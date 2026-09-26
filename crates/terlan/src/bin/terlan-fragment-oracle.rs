use std::env;
use std::fmt::Write as _;
use std::fs;
use std::process::ExitCode;

use terlan::compiler::syntax::lalrpop_boundary::{
    parse_lalrpop_expression, parse_lalrpop_pattern, parse_lalrpop_type,
};
use terlan::compiler::syntax::lalrpop_syntax::{LalrpopSyntaxNode, LalrpopSyntaxNodeKind};

fn normalized_kind(kind: LalrpopSyntaxNodeKind) -> &'static str {
    use LalrpopSyntaxNodeKind::*;
    match kind {
        Int => "int",
        Float => "float",
        String => "string",
        AtomLiteral => "atom_literal",
        BinaryLiteral => "binary_literal",
        BinaryLayout => "binary_layout",
        BinaryLayoutField => "binary_layout_field",
        RawMacro => "raw_macro",
        Binding => "binding",
        Group | Tuple => "group",
        List => "list",
        FixedArray => "fixed_array",
        Map => "map",
        MapField => "map_field",
        ListCons => "list_cons",
        ListComprehension => "list_comprehension",
        Generator => "generator",
        Unary => "unary",
        Binary => "binary",
        Cast => "cast",
        Type => "type",
        TypeUnion => "type_union",
        TypeArrow => "type_arrow",
        TypeExistential => "type_existential",
        TypeTuple | TypeMap => "type_aggregate",
        TypeField => "type_field",
        TypeList => "type_list",
        Pattern => "pattern",
        PatternTuple => "pattern_tuple",
        PatternList => "pattern_list",
        PatternListCons => "pattern_list_cons",
        PatternMap => "pattern_map",
        PatternField => "pattern_field",
        PatternConstructor => "pattern_constructor",
        Call => "call",
        Index => "index",
        IndexAssign => "index_assign",
        FieldAccess => "field_access",
        RecordAccess => "record_access",
        RecordUpdate => "record_update",
        Sequence => "sequence",
        Quote => "quote",
        Unquote => "unquote",
        Let => "let",
        Case => "case",
        Try => "try",
        If => "if",
        Clause => "clause",
        Lambda => "lambda",
        ConstructorChain => "constructor_chain",
        MacroCall => "macro_call",
        Module => "module",
        ModuleDeclaration => "module_declaration",
        ImportDeclaration => "import_declaration",
        ExportDeclaration => "export_declaration",
        ExportItem => "export_item",
        ConstantDeclaration => "constant_declaration",
        TypeDeclaration => "type_declaration",
        ValuedUnionArm => "valued_union_arm",
        StructDeclaration => "struct_declaration",
        ConstructorDeclaration => "constructor_declaration",
        ConstructorClause => "constructor_clause",
        TraitDeclaration => "trait_declaration",
        TraitImplementationDeclaration => "trait_implementation_declaration",
        TemplateDeclaration => "template_declaration",
        ConfigDeclaration => "config_declaration",
        ShapeDeclaration => "shape_declaration",
        FunctionDeclaration => "function_declaration",
        MethodDeclaration => "method_declaration",
        Receiver => "receiver",
        Parameter => "parameter",
        StructField => "struct_field",
        Annotation => "annotation",
        AnnotationValue => "annotation_value",
    }
}

fn rows(node: &LalrpopSyntaxNode, depth: usize, output: &mut String) {
    let _ = writeln!(
        output,
        "{depth}\t{}\t{}\t{}",
        normalized_kind(node.kind),
        node.span.start,
        node.span.end
    );
    for child in &node.children {
        rows(child, depth + 1, output);
    }
}

fn document(mode: &str, parsed: Result<LalrpopSyntaxNode, (String, usize, usize)>) -> String {
    let mut output = format!("schema terlan.self-host.fragment-syntax/v1\nmode\t{mode}\n");
    match parsed {
        Ok(root) => {
            output.push_str("valid\ttrue\nmessage_bytes\t0\ndepth\tkind\tstart\tend\n");
            rows(&root, 0, &mut output);
        }
        Err((message, _, _)) => {
            let _ = write!(
                output,
                "valid\tfalse\nmessage_bytes\t{}\ndepth\tkind\tstart\tend\n",
                message.len()
            );
        }
    }
    output
}

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(mode) = arguments.next() else {
        eprintln!("usage: terlan-fragment-oracle expression|pattern|type <input> <output>");
        return ExitCode::FAILURE;
    };
    let Some(input_path) = arguments.next() else {
        eprintln!("missing input path");
        return ExitCode::FAILURE;
    };
    let Some(output_path) = arguments.next() else {
        eprintln!("missing output path");
        return ExitCode::FAILURE;
    };
    let source = match fs::read_to_string(&input_path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{input_path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let parsed = match mode.as_str() {
        "expression" => parse_lalrpop_expression(&source)
            .map(|output| output.root)
            .map_err(|error| (error.message, error.span.start, error.span.end)),
        "pattern" => parse_lalrpop_pattern(&source)
            .map(|output| output.root)
            .map_err(|error| (error.message, error.span.start, error.span.end)),
        "type" => parse_lalrpop_type(&source)
            .map(|output| output.root)
            .map_err(|error| (error.message, error.span.start, error.span.end)),
        _ => {
            eprintln!("unknown fragment mode: {mode}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = fs::write(&output_path, document(&mode, parsed)) {
        eprintln!("{output_path}: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
