use super::{lalrpop_boundary::*, lalrpop_syntax::LalrpopSourceIndex, span::Span};

mod tests {
    use std::{fs, path::Path};

    use super::*;
    use crate::compiler::syntax::lalrpop_syntax::{
        LalrpopSyntaxNodeKind, LALRPOP_EXPRESSION_OUTPUT_SCHEMA, LALRPOP_MODULE_OUTPUT_SCHEMA,
        LALRPOP_PATTERN_OUTPUT_SCHEMA, LALRPOP_TYPE_OUTPUT_SCHEMA,
    };

    #[test]
    fn generated_boundary_preserves_token_spans_and_grammar_identity() {
        let source = "module app.Main.";
        let output = parse_lalrpop_token_output(source).expect("parse token boundary");
        assert_eq!(output.schema, LALRPOP_SYNTAX_OUTPUT_SCHEMA);
        assert!(!output.grammar_identity.is_empty());
        assert_eq!(
            output
                .tokens
                .iter()
                .map(|token| (token.terminal, token.span))
                .collect::<Vec<_>>(),
            vec![
                ("module", Span::new(0, 6)),
                ("atom", Span::new(7, 10)),
                (".", Span::new(10, 11)),
                ("upper-ident", Span::new(11, 15)),
                (".", Span::new(15, 16)),
            ]
        );
    }

    #[test]
    fn generated_boundary_preserves_canonical_lexer_errors() {
        let error = parse_lalrpop_token_output("module app.Main. \"")
            .expect_err("unterminated string must fail");
        assert_eq!(error.message, "unterminated string literal");
        assert_eq!(error.span, Span::new(17, 18));
    }

    #[test]
    fn generated_module_header_owns_module_path_and_span() {
        let source = "//! docs\nmodule app.web.Main.\n\npub run(): Int -> 1.";
        let output = parse_lalrpop_module_header(source).expect("parse module header");
        assert_eq!(output.module_name, "app.web.Main");
        assert_eq!(
            &source[output.span.start..output.span.end],
            "module app.web.Main."
        );
    }

    #[test]
    fn generated_module_header_rejects_missing_terminator_at_exact_span() {
        let source = "module app.Main";
        let error = parse_lalrpop_module_header(source).expect_err("missing dot must fail");
        assert_eq!(error.span, Span::new(source.len(), source.len()));
        assert!(error.message.contains("unexpected end of input"));
    }

    #[test]
    fn generated_expression_owns_precedence_and_spans() {
        let source = "A |> C or D and E == F + G * H";
        let output = parse_lalrpop_expression(source).expect("parse generated expression");
        assert_eq!(output.schema, LALRPOP_EXPRESSION_OUTPUT_SCHEMA);
        assert_eq!(output.root.text.as_deref(), Some("|>"));
        assert_eq!(output.root.span, Span::new(0, source.len()));
        let or = &output.root.children[1];
        assert_eq!(or.text.as_deref(), Some("or"));
        let and = &or.children[1];
        assert_eq!(and.text.as_deref(), Some("and"));
        let comparison = &and.children[1];
        assert_eq!(comparison.text.as_deref(), Some("=="));
        let addition = &comparison.children[1];
        assert_eq!(addition.text.as_deref(), Some("+"));
        assert_eq!(addition.children[1].text.as_deref(), Some("*"));
    }

    #[test]
    fn generated_expression_owns_postfix_collection_shape() {
        let source = "service.users()[index + 1]";
        let output = parse_lalrpop_expression(source).expect("parse generated postfix expression");
        assert_eq!(output.root.kind, LalrpopSyntaxNodeKind::Index);
        assert_eq!(output.root.children[0].kind, LalrpopSyntaxNodeKind::Call);
        assert_eq!(
            output.root.children[0].children[0].kind,
            LalrpopSyntaxNodeKind::FieldAccess
        );
        assert_eq!(output.root.span, Span::new(0, source.len()));
    }

    #[test]
    fn generated_expression_validation_rejects_plain_assignment() {
        let source = "value = value + 1";
        let error = parse_lalrpop_expression(source).expect_err("plain assignment must fail");
        assert!(error.message.contains("plain `=`"));
        assert_eq!(error.span, Span::new(0, source.len()));

        let indexed = parse_lalrpop_expression("values[index] = replacement")
            .expect("indexed assignment remains valid");
        assert_eq!(indexed.root.kind, LalrpopSyntaxNodeKind::IndexAssign);
    }

    #[test]
    fn generated_expression_covers_canonical_structural_core() {
        let cases = [
            ("42", LalrpopSyntaxNodeKind::Int),
            ("3.5", LalrpopSyntaxNodeKind::Float),
            ("\"ready\"", LalrpopSyntaxNodeKind::String),
            ("Atom[\"ready\"]", LalrpopSyntaxNodeKind::AtomLiteral),
            ("{first, second, third}", LalrpopSyntaxNodeKind::Tuple),
            ("[first, second]", LalrpopSyntaxNodeKind::List),
            ("[head | tail]", LalrpopSyntaxNodeKind::ListCons),
            ("#[255, 128, 0]", LalrpopSyntaxNodeKind::FixedArray),
            ("{name: \"Ada\", age: 42}", LalrpopSyntaxNodeKind::Map),
            ("-value", LalrpopSyntaxNodeKind::Unary),
            ("Value as Int", LalrpopSyntaxNodeKind::Cast),
            ("quote value + 1", LalrpopSyntaxNodeKind::Quote),
            ("unquote(value)", LalrpopSyntaxNodeKind::Unquote),
            ("make(1, nested(2))", LalrpopSyntaxNodeKind::Call),
            ("first; second", LalrpopSyntaxNodeKind::Sequence),
        ];
        for (source, expected_kind) in cases {
            crate::terlan_syntax::parser::parse_terlan_expr(source)
                .unwrap_or_else(|error| panic!("canonical parser rejected {source}: {error:?}"));
            let generated = parse_lalrpop_expression(source)
                .unwrap_or_else(|error| panic!("generated parser rejected {source}: {error:?}"));
            assert_eq!(
                generated.root.kind, expected_kind,
                "generated shape drift for {source}"
            );
            assert_eq!(generated.root.span, Span::new(0, source.len()));
        }
    }

    #[test]
    fn generated_expression_covers_control_flow_core() {
        let cases = [
            ("let value = 1; value + 1", LalrpopSyntaxNodeKind::Let),
            (
                "case value { ready -> 1; other -> 0; }",
                LalrpopSyntaxNodeKind::Case,
            ),
            (
                "case value { Err(reason) -> false; Ok(result) -> result; }",
                LalrpopSyntaxNodeKind::Case,
            ),
            ("if { ready -> 1; other -> 0; }", LalrpopSyntaxNodeKind::If),
            (
                "(value, state) -> value + state",
                LalrpopSyntaxNodeKind::Lambda,
            ),
            (
                "?assert_equal(actual, expected)",
                LalrpopSyntaxNodeKind::MacroCall,
            ),
        ];
        for (source, expected_kind) in cases {
            crate::terlan_syntax::parser::parse_terlan_expr(source)
                .unwrap_or_else(|error| panic!("canonical parser rejected {source}: {error:?}"));
            let generated = parse_lalrpop_expression(source)
                .unwrap_or_else(|error| panic!("generated parser rejected {source}: {error:?}"));
            assert_eq!(generated.root.kind, expected_kind, "shape for {source}");
            assert_eq!(generated.root.span, Span::new(0, source.len()));
        }
    }

    #[test]
    fn generated_type_output_covers_shape_domain_core() {
        let cases = [
            ("Int", LalrpopSyntaxNodeKind::Type),
            ("Int | String", LalrpopSyntaxNodeKind::TypeUnion),
            ("[Int]", LalrpopSyntaxNodeKind::TypeList),
            ("{Int, String}", LalrpopSyntaxNodeKind::TypeTuple),
            ("{name: String, age: Int}", LalrpopSyntaxNodeKind::TypeMap),
            (
                "exists Value. {value: Value}",
                LalrpopSyntaxNodeKind::TypeExistential,
            ),
        ];
        for (source, expected_kind) in cases {
            let output = parse_lalrpop_type(source)
                .unwrap_or_else(|error| panic!("generated type rejected {source}: {error:?}"));
            assert_eq!(output.schema, LALRPOP_TYPE_OUTPUT_SCHEMA);
            assert_eq!(output.root.kind, expected_kind, "type shape for {source}");
            assert_eq!(output.root.span, Span::new(0, source.len()));
        }
    }

    #[test]
    fn generated_pattern_output_covers_shape_domain_core() {
        let cases = [
            ("_", LalrpopSyntaxNodeKind::Pattern),
            ("value", LalrpopSyntaxNodeKind::Pattern),
            ("Some(value)", LalrpopSyntaxNodeKind::PatternConstructor),
            ("{first, second}", LalrpopSyntaxNodeKind::PatternTuple),
            ("[first, second]", LalrpopSyntaxNodeKind::PatternList),
            ("[head | tail]", LalrpopSyntaxNodeKind::PatternListCons),
            ("{name: value}", LalrpopSyntaxNodeKind::PatternMap),
            (
                "User{name: value}",
                LalrpopSyntaxNodeKind::PatternConstructor,
            ),
            ("Atom[\"ready\"]", LalrpopSyntaxNodeKind::Pattern),
        ];
        for (source, expected_kind) in cases {
            crate::terlan_syntax::parser::parse_terlan_pattern(source).unwrap_or_else(|error| {
                panic!("canonical pattern parser rejected {source}: {error:?}")
            });
            let output = parse_lalrpop_pattern(source)
                .unwrap_or_else(|error| panic!("generated pattern rejected {source}: {error:?}"));
            assert_eq!(output.schema, LALRPOP_PATTERN_OUTPUT_SCHEMA);
            assert_eq!(
                output.root.kind, expected_kind,
                "pattern shape for {source}"
            );
            assert_eq!(output.root.span, Span::new(0, source.len()));
        }
    }

    #[test]
    fn generated_module_output_owns_core_declarations() {
        let source = r#"
//! module documentation
module generated.Sample.

import std.http.Client.
import type std.core.Result.
import std.core.Option.{Some}.
pub const PORT: Int = 8080.
pub type UserId = Int.
pub opaque type Handle[T].
pub type UserIds = std.collections.List.List[UserId].
pub struct User { name: String, age: Int }.
pub identity[T](value: T): T -> value.
pub constrained[T](value: T)[Equal[T]]: Bool -> true.
pub (user: User) age(): Int -> user.age.
@test
pub run(value: Int): Int ->
    case value { 0 -> 1; other -> other; }.
"#;
        let canonical =
            crate::terlan_syntax::parser::parse_module(source).expect("canonical module parse");
        let output = parse_lalrpop_module_syntax(source).expect("generated module parse");
        assert_eq!(output.schema, LALRPOP_MODULE_OUTPUT_SCHEMA);
        assert_eq!(output.module_name, canonical.name);
        assert_eq!(output.root.children.len(), canonical.declarations.len() + 1);
        assert_eq!(
            output
                .root
                .children
                .iter()
                .skip(1)
                .map(|node| node.kind)
                .collect::<Vec<_>>(),
            vec![
                LalrpopSyntaxNodeKind::ImportDeclaration,
                LalrpopSyntaxNodeKind::ImportDeclaration,
                LalrpopSyntaxNodeKind::ImportDeclaration,
                LalrpopSyntaxNodeKind::ConstantDeclaration,
                LalrpopSyntaxNodeKind::TypeDeclaration,
                LalrpopSyntaxNodeKind::TypeDeclaration,
                LalrpopSyntaxNodeKind::TypeDeclaration,
                LalrpopSyntaxNodeKind::StructDeclaration,
                LalrpopSyntaxNodeKind::FunctionDeclaration,
                LalrpopSyntaxNodeKind::FunctionDeclaration,
                LalrpopSyntaxNodeKind::MethodDeclaration,
                LalrpopSyntaxNodeKind::FunctionDeclaration,
            ]
        );
        assert_eq!(
            output
                .root
                .children
                .last()
                .and_then(|function| function.children.first())
                .map(|node| node.kind),
            Some(LalrpopSyntaxNodeKind::Annotation)
        );
    }

    #[test]
    fn generated_module_migration_scorecard_tracks_full_corpus() {
        use std::collections::BTreeMap;

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut paths = Vec::new();
        collect_terlan_sources(&root, &mut paths);
        paths.sort();
        let mut canonical_accepted = 0usize;
        let mut generated_accepted = 0usize;
        let mut failure_classes = BTreeMap::<String, (usize, String)>::new();
        for path in paths {
            let source = fs::read_to_string(&path).expect("read Terlan corpus source");
            let Ok(canonical) = crate::terlan_syntax::parser::parse_module(&source) else {
                continue;
            };
            canonical_accepted += 1;
            match parse_lalrpop_module_syntax(&source) {
                Ok(generated) => {
                    generated_accepted += 1;
                    assert_eq!(
                        generated.module_name,
                        canonical.name,
                        "module identity drift for {}",
                        path.display()
                    );
                }
                Err(error) => {
                    let source_index = LalrpopSourceIndex::new(&source);
                    let message_class = error
                        .message
                        .split("; expected")
                        .next()
                        .unwrap_or(error.message.as_str())
                        .to_string();
                    let token = source_index.text(&source, error.span.start, error.span.end);
                    let class = format!("{message_class} `{token}`");
                    let line_index = source
                        .chars()
                        .take(error.span.start)
                        .filter(|character| *character == '\n')
                        .count();
                    let context = source.lines().nth(line_index).unwrap_or("");
                    let sample = format!(
                        "{}:{} `{}` ({})",
                        path.display(),
                        error.span.start,
                        context.trim(),
                        error.message,
                    );
                    let entry = failure_classes.entry(class).or_insert_with(|| (0, sample));
                    entry.0 += 1;
                }
            }
        }
        eprintln!("generated module migration: {generated_accepted}/{canonical_accepted} accepted");
        let mut failure_classes = failure_classes.into_iter().collect::<Vec<_>>();
        failure_classes.sort_by(|left, right| {
            right
                .1
                 .0
                .cmp(&left.1 .0)
                .then_with(|| left.0.cmp(&right.0))
        });
        for (class, (count, sample)) in failure_classes.iter().take(16) {
            eprintln!("  {count:4} {class}: {sample}");
        }
        assert!(canonical_accepted >= 1_000);
        assert_eq!(
            generated_accepted, canonical_accepted,
            "generated declaration grammar must accept every canonical corpus file"
        );
    }

    #[test]
    fn generated_module_header_matches_every_canonical_corpus_file() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut paths = Vec::new();
        collect_terlan_sources(&root, &mut paths);
        paths.sort();
        let mut accepted = 0usize;
        for path in paths {
            let source = fs::read_to_string(&path).expect("read Terlan corpus source");
            let Ok(canonical) = crate::terlan_syntax::parser::parse_module(&source) else {
                continue;
            };
            accepted += 1;
            let generated = parse_lalrpop_module_header(&source).unwrap_or_else(|error| {
                panic!(
                    "generated module header rejected {} at {:?}: {}",
                    path.display(),
                    error.span,
                    error.message
                )
            });
            assert_eq!(
                generated.module_name,
                canonical.name,
                "module identity drift for {}",
                path.display()
            );
        }
        assert!(
            accepted >= 1_000,
            "expected broad accepted syntax corpus, found {accepted}"
        );
    }

    #[test]
    fn generated_interface_parser_accepts_repository_interface_corpus() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut paths = Vec::new();
        collect_interface_sources(&root, &mut paths);
        paths.sort();
        let mut parsed = 0usize;
        for path in &paths {
            let source = fs::read_to_string(path).expect("read Terlan interface source");
            if !source.lines().any(|line| {
                let line = line.trim();
                line.starts_with("module ") && line.ends_with('.')
            }) {
                continue;
            }
            parsed += 1;
            crate::terlan_syntax::parser::parse_interface_module(&source).unwrap_or_else(|error| {
                panic!(
                    "generated interface parser rejected {} at {:?}: {}",
                    path.display(),
                    error.span,
                    error.message,
                )
            });
        }
        assert!(
            parsed >= 1_000,
            "expected broad interface corpus, found {}",
            parsed,
        );
    }

    fn collect_terlan_sources(directory: &Path, output: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(directory).expect("read corpus directory") {
            let entry = entry.expect("read corpus entry");
            let path = entry.path();
            if path.is_dir() {
                if !matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some(".git" | "target")
                ) {
                    collect_terlan_sources(&path, output);
                }
            } else if path
                .extension()
                .is_some_and(|extension| extension == "terl")
            {
                output.push(path);
            }
        }
    }

    fn collect_interface_sources(directory: &Path, output: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(directory).expect("read interface corpus directory") {
            let entry = entry.expect("read interface corpus entry");
            let path = entry.path();
            if path.is_dir() {
                if !matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some(".git" | "target")
                ) {
                    collect_interface_sources(&path, output);
                }
            } else if path
                .extension()
                .is_some_and(|extension| extension == "terli" || extension == "typi")
            {
                output.push(path);
            }
        }
    }
}
