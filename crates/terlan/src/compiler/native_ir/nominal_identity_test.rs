use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{
    lower_syntax_module_output_to_core, type_check_syntax_module_output, CoreImport,
    CoreImportKind, CoreModule, CoreType,
};

use super::qualify_application_nominal_types;

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
