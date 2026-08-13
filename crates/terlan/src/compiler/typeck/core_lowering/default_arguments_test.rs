use crate::compiler::hir::{FunctionSignature, ModuleInterface, ParamSignature, ResolvedModule};
use crate::terlan_syntax::{parse_module_as_syntax_output, SyntaxDeclarationPayload};

use super::*;

fn parameter(name: &str, default: Option<&str>) -> ParamSignature {
    ParamSignature {
        name: name.to_string(),
        annotation: "Int".to_string(),
        is_mutable: false,
        default: default.map(|source| {
            crate::terlan_syntax::parse_expr_as_syntax_output(source)
                .expect("valid test default expression")
        }),
        default_text: default.map(str::to_string),
    }
}

fn resolved_with_import() -> ResolvedModule {
    let mut provider = ModuleInterface {
        module: "sample.Options".to_string(),
        ..ModuleInterface::default()
    };
    provider.functions.insert(
        ("configure".to_string(), 3),
        FunctionSignature {
            name: "configure".to_string(),
            generic_params: Vec::new(),
            params: vec![
                parameter("first", Some("1")),
                parameter("second", Some("2")),
                parameter("third", Some("3")),
            ],
            return_type: "Int".to_string(),
            generic_bounds: Vec::new(),
            receiver_method: false,
            receiver_mutable: false,
            public: true,
            pure: false,
            docs: Vec::new(),
        },
    );
    ResolvedModule {
        name: "sample.Caller".to_string(),
        interface_map: HashMap::from([("sample.Options".to_string(), provider)]),
        ..empty_resolved("sample.Caller")
    }
}

fn empty_resolved(name: &str) -> ResolvedModule {
    ResolvedModule {
        name: name.to_string(),
        function_symbols: HashMap::new(),
        local_type_names: HashMap::new(),
        imported_types: HashMap::new(),
        imported_traits: HashMap::new(),
        imported_constants: HashMap::new(),
        interface_map: HashMap::new(),
        interface: ModuleInterface {
            module: name.to_string(),
            ..ModuleInterface::default()
        },
        diagnostics: Vec::new(),
    }
}

#[test]
fn imported_named_defaults_materialize_in_declaration_order() {
    let mut module = parse_module_as_syntax_output(
        "module sample.Caller.\n\
         pub run(): Int -> sample.Options.configure(third = 9).",
    )
    .expect("parse caller");
    materialize_default_call_arguments(&mut module, &resolved_with_import());
    let SyntaxDeclarationPayload::Function { clauses, .. } = &module.declarations[0].payload else {
        panic!("function declaration");
    };
    let call = &clauses[0].body;
    assert_eq!(call.arity, 3);
    assert_eq!(call.arg_names, vec![None, None, None]);
    assert_eq!(call.children[1].text.as_deref(), Some("1"));
    assert_eq!(call.children[2].text.as_deref(), Some("2"));
    assert_eq!(call.children[3].text.as_deref(), Some("9"));
}

#[test]
fn local_defaults_materialize_without_touching_unknown_calls() {
    let mut module = parse_module_as_syntax_output(
        "module sample.Local.\n\
         pub configure(first: Int = 1, second: Int = 2): Int -> first + second.\n\
         pub run(): Int -> configure(second = 7).",
    )
    .expect("parse local functions");
    let resolved = empty_resolved("sample.Local");
    materialize_default_call_arguments(&mut module, &resolved);
    let SyntaxDeclarationPayload::Function { clauses, .. } = &module.declarations[1].payload else {
        panic!("function declaration");
    };
    assert_eq!(clauses[0].body.arity, 2);
    assert_eq!(clauses[0].body.children[1].text.as_deref(), Some("1"));
    assert_eq!(clauses[0].body.children[2].text.as_deref(), Some("7"));
}
