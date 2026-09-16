use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{
    lower_syntax_module_output_to_core, type_check_syntax_module_output, CoreImport,
    CoreImportKind, CoreModule, CoreType,
};

use super::qualify_application_nominal_types;

#[test]
fn compiler_collection_qualification_keeps_one_nested_abi_identity() {
    for (name, expected) in [
        ("std.collections.Map.Map", "Map"),
        ("std.collections.Set.Set", "Set"),
        ("package.Custom.Map", "package.Custom.Map"),
        ("package.Custom.Set", "package.Custom.Set"),
    ] {
        let mut core = checked_core("module app.Collection. pub values(): List[Int] -> [].");
        let args = if name.ends_with(".Map") {
            vec![CoreType::String, CoreType::Int]
        } else {
            vec![CoreType::Int]
        };
        core.functions[0].core_return_type = Some(CoreType::List(Box::new(CoreType::Apply {
            constructor: name.to_string(),
            args: args.clone(),
        })));
        qualify_application_nominal_types(std::slice::from_mut(&mut core));
        assert_eq!(
            core.functions[0].core_return_type,
            Some(CoreType::List(Box::new(CoreType::Apply {
                constructor: expected.to_string(),
                args,
            })))
        );
    }
}

fn checked_core(source: &str) -> CoreModule {
    let syntax = parse_module_as_syntax_output(source).expect("parse module");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

fn provider(module: &str) -> CoreModule {
    checked_core(&format!(
        "module {module}.\n\npub struct Classification {{ name: String }}.\n"
    ))
}

fn consumer(imports: &[&str]) -> CoreModule {
    let mut core = checked_core("module app.Consumer.\n\npub values(): List[String] -> [].\n");
    core.imports.extend(imports.iter().map(|module| CoreImport {
        module: (*module).to_string(),
        kind: CoreImportKind::TypeModule,
    }));
    core.functions[0].core_return_type = Some(CoreType::List(Box::new(CoreType::Named(
        "Classification".to_string(),
    ))));
    core
}

#[test]
fn uniquely_imported_nominal_inside_list_gets_application_identity() {
    let mut cores = vec![provider("package.Types"), consumer(&["package.Types"])];
    qualify_application_nominal_types(&mut cores);

    assert_eq!(
        cores[1].functions[0].core_return_type,
        Some(CoreType::List(Box::new(CoreType::Named(
            "package.Types.Classification".to_string()
        ))))
    );
}

#[test]
fn ambiguous_imported_nominal_remains_unqualified() {
    let mut cores = vec![
        provider("package.Left"),
        provider("package.Right"),
        consumer(&["package.Left", "package.Right"]),
    ];
    qualify_application_nominal_types(&mut cores);

    assert_eq!(
        cores[2].functions[0].core_return_type,
        Some(CoreType::List(Box::new(CoreType::Named(
            "Classification".to_string()
        ))))
    );
}
