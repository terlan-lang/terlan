use std::collections::HashMap;

use super::{
    expand_syntax_shape_imports, resolve_syntax_module_output_with_interfaces,
    syntax_module_output_to_interface,
};
use crate::terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output,
    SyntaxDeclarationPayload, SyntaxExprKind, SyntaxPatternKind, SyntaxPatternOutput,
};
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

fn provider_interface(source: &str) -> (String, super::ModuleInterface) {
    let module = parse_interface_module_as_syntax_output(source)
        .expect("provider shape interface should parse");
    let name = module.module_name.clone();
    (name, syntax_module_output_to_interface(&module))
}

fn contains_constructor(pattern: &SyntaxPatternOutput, names: &[&str]) -> bool {
    (pattern.kind == SyntaxPatternKind::Constructor
        && pattern
            .text
            .as_deref()
            .is_some_and(|name| names.contains(&name)))
        || pattern
            .children
            .iter()
            .any(|child| contains_constructor(child, names))
        || pattern
            .fields
            .iter()
            .any(|field| contains_constructor(&field.value, names))
}

#[test]
fn imported_shape_expansion_normalizes_selected_alias_and_nested_guard() {
    let (provider_name, provider) = provider_interface(
        "module provider.Shapes.\n\
         pub shape Positive(value) = value where value > 0.\n\
         pub shape Tagged(value) = {Atom[\"ok\"], Positive(value)}.\n",
    );
    let mut interfaces = HashMap::new();
    interfaces.insert(provider_name, provider);
    let mut consumer = parse_module_as_syntax_output(
        "module shape_consumer.\n\
         import provider.Shapes.{Tagged as Success}.\n\
         pub read(input: Dynamic): Int ->\n\
             case input { Success(value) -> value; _ -> 0 }.\n",
    )
    .expect("shape consumer should parse");

    expand_syntax_shape_imports(&mut consumer, &interfaces)
        .expect("selected imported shape should expand");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &consumer.declarations[1].payload
    else {
        panic!("expected consumer function");
    };
    let case_clause = &clauses[0].body.clauses[0];
    assert!(!contains_constructor(
        &case_clause.patterns[0],
        &["Success", "Tagged", "Positive"]
    ));
    let guard = case_clause.guard.as_ref().expect("nested provider guard");
    assert_eq!(guard.kind, SyntaxExprKind::BinaryOp);
    assert_eq!(guard.operator.as_deref(), Some(">"));

    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&consumer, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    let core = lower_syntax_module_output_to_core(&consumer, &resolved);
    assert!(
        core.functions
            .iter()
            .find(|function| function.name == "read")
            .is_some_and(|function| !function.clauses.is_empty()),
        "imported shape function should lower to CoreIR"
    );
}

#[test]
fn imported_shape_expansion_supports_wildcard_imports() {
    let (provider_name, provider) = provider_interface(
        "module provider.WildcardShapes.\n\
         pub shape Pair(value) = {Atom[\"pair\"], value}.\n",
    );
    let mut interfaces = HashMap::new();
    interfaces.insert(provider_name, provider);
    let mut consumer = parse_module_as_syntax_output(
        "module wildcard_shape_consumer.\n\
         import provider.WildcardShapes.{*}.\n\
         pub read(input: Dynamic): Int ->\n\
             case input { Pair(value) -> value; _ -> 0 }.\n",
    )
    .expect("wildcard shape consumer should parse");

    expand_syntax_shape_imports(&mut consumer, &interfaces)
        .expect("wildcard imported shape should expand");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &consumer.declarations[1].payload
    else {
        panic!("expected consumer function");
    };
    let pattern = &clauses[0].body.clauses[0].patterns[0];
    assert_eq!(pattern.kind, SyntaxPatternKind::Tuple);
    assert!(!contains_constructor(pattern, &["Pair"]));
}

#[test]
fn imported_shape_expansion_rejects_alias_called_as_runtime_value() {
    let (provider_name, provider) = provider_interface(
        "module provider.RuntimeShapes.\n\
         pub shape Pair(value) = {Atom[\"pair\"], value}.\n",
    );
    let mut interfaces = HashMap::new();
    interfaces.insert(provider_name, provider);
    let mut consumer = parse_module_as_syntax_output(
        "module runtime_shape_consumer.\n\
         import provider.RuntimeShapes.{Pair as Match}.\n\
         pub build(value: Int): Dynamic -> Match(value).\n",
    )
    .expect("runtime shape call should reach imported shape expansion");

    let error = expand_syntax_shape_imports(&mut consumer, &interfaces)
        .expect_err("imported shape aliases must not construct runtime values");
    assert_eq!(
        error,
        crate::terlan_syntax::ebnf::EbnfCompileError::Serialize(
            "shape `Match` is compile-time pattern-only and cannot be called as a runtime value"
                .to_string()
        )
    );
}

#[test]
fn imported_shape_expansion_rejects_ambiguous_local_aliases() {
    let providers = [
        provider_interface(
            "module provider.LeftShapes.\n\
             pub shape Pair(value) = {Atom[\"left\"], value}.\n",
        ),
        provider_interface(
            "module provider.RightShapes.\n\
             pub shape Pair(value) = {Atom[\"right\"], value}.\n",
        ),
    ];
    let interfaces = providers.into_iter().collect::<HashMap<_, _>>();
    let mut consumer = parse_module_as_syntax_output(
        "module ambiguous_shape_consumer.\n\
         import provider.LeftShapes.{Pair as Match}.\n\
         import provider.RightShapes.{Pair as Match}.\n\
         pub read(input: Dynamic): Int ->\n\
             case input { Match(value) -> value; _ -> 0 }.\n",
    )
    .expect("ambiguous shape consumer should parse");

    let error = expand_syntax_shape_imports(&mut consumer, &interfaces)
        .expect_err("ambiguous imported shape aliases must fail");
    assert_eq!(
        error,
        crate::terlan_syntax::ebnf::EbnfCompileError::Serialize(
            "ambiguous imported shape alias `Match`: `provider.LeftShapes.Pair` and `provider.RightShapes.Pair`"
                .to_string()
        )
    );
}

#[test]
fn imported_shape_expansion_rejects_recursive_provider_shapes() {
    let (provider_name, provider) = provider_interface(
        "module provider.RecursiveShapes.\n\
         pub shape Left(value) = Right(value).\n\
         pub shape Right(value) = Left(value).\n",
    );
    let mut interfaces = HashMap::new();
    interfaces.insert(provider_name, provider);
    let mut consumer = parse_module_as_syntax_output(
        "module recursive_shape_consumer.\n\
         import provider.RecursiveShapes.{Left}.\n\
         pub read(input: Dynamic): Int ->\n\
             case input { Left(_) -> 1; _ -> 0 }.\n",
    )
    .expect("recursive shape consumer should parse");

    let error = expand_syntax_shape_imports(&mut consumer, &interfaces)
        .expect_err("recursive provider shapes must fail");
    assert!(
        format!("{error:?}").contains("recursive shape expansion: Left -> Right -> Left"),
        "error: {error:?}"
    );
}
