//! Shared compiler/editor examples tied to named canonical EBNF rules.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::{canonical_terlan_syntax_contract, parse_module, parse_terlan_expr, parse_tree::*};

const MANIFEST: &str =
    include_str!("../../../../../docs/grammar/fixtures/contract/syntax_conformance.json");
const CORPUS: &str =
    include_str!("../../../../../tree-sitter-terlan/test/corpus/syntax_contract.txt");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
enum Compiler {
    Accept,
    Reject,
}

#[derive(Deserialize, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
enum Editor {
    Accept,
    Recover,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    rule: String,
    compiler: Compiler,
    ebnf: Compiler,
    contextual: Option<String>,
    editor: Editor,
    shape: Option<String>,
    diagnostic: Option<String>,
    divergence: Option<String>,
}

fn corpus_cases() -> BTreeMap<&'static str, (&'static str, &'static str)> {
    let parts: Vec<_> = CORPUS.split("==================\n").collect();
    assert!(parts[0].trim().is_empty(), "unexpected corpus preamble");
    let mut cases = BTreeMap::new();
    let mut pairs = parts[1..].chunks_exact(2);
    for pair in &mut pairs {
        let id = pair[0].trim();
        let (source, tree) = pair[1]
            .split_once("\n---\n")
            .expect("source/tree separator");
        assert!(
            tree.trim().starts_with("(source_file"),
            "{id}: missing editor tree"
        );
        assert!(
            cases.insert(id, (source.trim(), tree)).is_none(),
            "duplicate corpus ID {id}"
        );
    }
    assert!(pairs.remainder().is_empty(), "incomplete corpus case");
    cases
}

#[test]
fn shared_syntax_conformance_corpus_matches_contract() {
    let manifest: Manifest = serde_json::from_str(MANIFEST).expect("strict conformance manifest");
    assert_eq!(manifest.schema, 2);
    assert!(!manifest.cases.is_empty());
    let grammar = canonical_terlan_syntax_contract().expect("canonical EBNF");
    let recognizer = super::ebnf_recognizer::Recognizer::new(&grammar).expect("executable EBNF");
    let mut corpus = corpus_cases();
    let mut failures = Vec::new();
    for case in manifest.cases {
        assert!(
            grammar.rule(&case.rule).is_some(),
            "{}: unknown rule {}",
            case.id,
            case.rule
        );
        let (source, tree) = corpus
            .remove(case.id.as_str())
            .expect("unique manifest ID with a source case");
        assert_eq!(
            case.ebnf != case.compiler,
            case.contextual
                .as_ref()
                .is_some_and(|reason| !reason.trim().is_empty()),
            "{}: explain every grammar/contextual difference",
            case.id
        );
        match recognizer.recognizes("SyntaxSpec", source) {
            Ok(accepted) if accepted == (case.ebnf == Compiler::Accept) => {}
            actual => failures.push(format!(
                "{} [{}]: EBNF expected {:?}, found {actual:?}",
                case.id, case.rule, case.ebnf
            )),
        }
        let editor_recovers = tree.contains("(ERROR") || tree.contains("(MISSING");
        assert_eq!(
            editor_recovers,
            case.editor == Editor::Recover,
            "{}: editor expectation",
            case.id
        );
        let differs = (case.compiler == Compiler::Accept) != (case.editor == Editor::Accept);
        assert_eq!(
            differs,
            case.divergence
                .as_ref()
                .is_some_and(|reason| !reason.trim().is_empty()),
            "{}: explain every editor/compiler difference",
            case.id
        );
        assert_eq!(
            case.shape.is_some(),
            case.compiler == Compiler::Accept,
            "{}: accepted cases require an AST shape",
            case.id
        );
        match parse_module(source) {
            Ok(module) if case.compiler == Compiler::Accept => {
                assert_eq!(module.name, "contract.Cases", "{}", case.id);
                let actual = module_shape(&module);
                if Some(&actual) != case.shape.as_ref() {
                    failures.push(format!(
                        "{} [{}]: expected {:?}, found {actual}",
                        case.id, case.rule, case.shape
                    ));
                }
            }
            Err(error) if case.compiler == Compiler::Reject => {
                if let Some(expected) = case.diagnostic {
                    assert!(
                        error.message.contains(&expected),
                        "{}: {}",
                        case.id,
                        error.message
                    );
                }
            }
            actual => failures.push(format!(
                "{} [{}]: expected {:?}, found {actual:?}",
                case.id, case.rule, case.compiler
            )),
        }
    }
    assert!(
        corpus.is_empty(),
        "unlisted shared cases: {:?}",
        corpus.keys()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn module_shape(module: &Module) -> String {
    match module.declarations.as_slice() {
        [] => "module".into(),
        [Decl::TraitImpl(_)] => "impl".into(),
        [Decl::Type(_)] => "type".into(),
        [Decl::Function(function)] if function.clauses.len() == 1 => {
            expression_shape(&function.clauses[0].body)
        }
        declarations => panic!("add an explicit shape assertion for {declarations:?}"),
    }
}

fn expression_shape(expr: &Expr) -> String {
    match expr {
        Expr::Var(name) | Expr::Atom(name) => format!("name({name})"),
        Expr::List(items) => format!("list({})", items.len()),
        Expr::RawMacro {
            name,
            type_args,
            interpolations,
            raw,
        } => format!(
            "raw({name},{},{},{raw:?})",
            type_args.len(),
            interpolations.len()
        ),
        Expr::HtmlBlock(html) => format!("html({:?})", html.raw),
        Expr::ListCons(head, tail) => format!(
            "cons({},{})",
            expression_shape(head),
            expression_shape(tail)
        ),
        Expr::ListComprehension {
            generators, guards, ..
        } => format!("comprehension({},{})", generators.len(), guards.len()),
        Expr::Call {
            callee,
            remote,
            type_args,
            args,
            arg_names,
            ..
        } => {
            let (Expr::Atom(name) | Expr::Var(name)) = callee.as_ref() else {
                panic!("expected named call")
            };
            let qualified = remote
                .as_ref()
                .map_or_else(|| name.clone(), |module| format!("{module}.{name}"));
            format!(
                "call({qualified},{},{},{})",
                type_args.len(),
                args.len(),
                arg_names.iter().filter(|name| name.is_some()).count()
            )
        }
        Expr::BinaryOp { op, left, right } => format!(
            "{op:?}({},{})",
            expression_shape(left),
            expression_shape(right)
        ),
        Expr::Cast { expr, target_type } => {
            format!("cast({},{})", expression_shape(expr), target_type.text)
        }
        other => panic!("add an explicit shape assertion for {other:?}"),
    }
}

#[test]
fn conformance_shapes_distinguish_association_and_call_identity() {
    for (left, right) in [
        ("a < b < c", "a < (b < c)"),
        ("Console.println(1)", "Other.println(1)"),
        ("f(1)", "f(value = 1)"),
    ] {
        assert_ne!(
            expression_shape(&parse_terlan_expr(left).unwrap()),
            expression_shape(&parse_terlan_expr(right).unwrap())
        );
    }
}
