use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{
    lower_syntax_module_output_to_core, type_check_syntax_module_output, CoreImport,
    CoreImportKind, CoreModule, CoreType,
};

use super::qualify_application_nominal_types;

#[test]
fn nested_record_pattern_identity_follows_the_same_import_rules_as_its_type() {
    use crate::terlan_typeck::{CoreCaseClause, CoreExpr, CorePattern};
    for (imports, expected) in [
        (vec!["package.Left"], "package.Left.Classification"),
        (vec!["package.Left", "package.Right"], "Classification"),
    ] {
        let mut caller = consumer(&imports);
        caller.functions[0].clauses[0].body.core_expr = Some(CoreExpr::Case {
            scrutinee: Box::new(CoreExpr::List(vec![])),
            clauses: vec![CoreCaseClause {
                pattern: CorePattern::List(vec![CorePattern::Record {
                    name: "Classification".into(),
                    fields: vec![],
                }]),
                guard: None,
                body: CoreExpr::Int(1),
            }],
        });
        let mut cores = vec![provider("package.Left"), provider("package.Right"), caller];
        qualify_application_nominal_types(&mut cores);
        let Some(CoreExpr::Case { clauses, .. }) = &cores[2].functions[0].clauses[0].body.core_expr
        else {
            panic!("case expression");
        };
        assert_eq!(
            clauses[0].pattern,
            CorePattern::List(vec![CorePattern::Record {
                name: expected.into(),
                fields: vec![],
            }])
        );
    }
}

#[test]
fn local_record_patterns_lower_with_canonical_receiver_identity() {
    let core = checked_core(
        "module app.Records.
         pub struct Item { name: String }.
         pub read(items: List[Item]): String ->
             case items { [Item {name: value}] -> value; _ -> \"none\" }.",
    );
    super::super::NativeModule::lower_application(&[&core])
        .expect("nested record pattern identifies the same managed record as its input");
}

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

#[test]
fn record_union_patterns_and_construction_share_the_discriminated_layout() {
    let core = checked_core(
        "module app.RecordUnion.
         pub type Empty.
         pub struct Item { value: Int }.
         pub type Outcome = Empty | Item.
         pub read(item: Outcome): Int -> case item { Item {value: value} -> value; _ -> 0 }.
         pub produce(): Outcome -> Item {value: 42}.
         pub answer(): Int -> read(produce()).",
    );
    super::super::NativeModule::lower_application(&[&core])
        .expect("named record variants must lower through the closed union layout");
}

#[test]
fn record_union_pattern_rejects_a_foreign_nominal_identity() {
    use crate::terlan_typeck::{CoreExpr, CorePattern};
    let mut core = checked_core(
        "module app.RecordUnion.
         pub type Empty.
         pub struct Item { value: Int }.
         pub type Outcome = Empty | Item.
         pub read(item: Outcome): Int -> case item { Item {value: value} -> value; _ -> 0 }.",
    );
    let Some(CoreExpr::Case { clauses, .. }) = &mut core.functions[0].clauses[0].body.core_expr
    else {
        panic!("case")
    };
    let CorePattern::Record { name, .. } = &mut clauses[0].pattern else {
        panic!("record")
    };
    *name = "foreign.Item".into();
    let error = super::super::NativeModule::lower_application(&[&core]).unwrap_err();
    assert!(
        error.to_string().contains("native_ir.record_identity"),
        "{error}"
    );
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
