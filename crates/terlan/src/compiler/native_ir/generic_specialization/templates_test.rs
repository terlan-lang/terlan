use super::*;
use crate::terlan_typeck::{CoreMapTypeField, CoreStructTypeField, CoreTupleTypeElem};

fn field(name: &str, ty: CoreType) -> CoreStructTypeField {
    CoreStructTypeField {
        name: name.into(),
        ty,
        is_private: false,
    }
}

#[test]
fn nominal_field_projection_substitutes_generic_arguments() {
    let mut templates = CallableTemplates::default();
    templates.nominal_types.insert(
        "app.Batch".into(),
        NominalTypeTemplate {
            params: vec!["State".into()],
            body: CoreType::Struct {
                name: "app.Batch".into(),
                fields: vec![field(
                    "state",
                    CoreType::Apply {
                        constructor: "app.State".into(),
                        args: vec![CoreType::Named("State".into())],
                    },
                )],
            },
        },
    );
    assert_eq!(
        templates.field_type(
            &CoreType::Apply {
                constructor: "Batch".into(),
                args: vec![CoreType::Int],
            },
            "state",
            "app",
        ),
        Some(CoreType::Apply {
            constructor: "app.State".into(),
            args: vec![CoreType::Int],
        })
    );
}

#[test]
fn nominal_field_projection_handles_concrete_and_invalid_types() {
    let mut templates = CallableTemplates::default();
    templates.nominal_types.insert(
        "app.Execution".into(),
        NominalTypeTemplate {
            params: vec![],
            body: CoreType::Struct {
                name: "app.Execution".into(),
                fields: vec![field("sequence", CoreType::Int)],
            },
        },
    );
    templates.nominal_types.insert(
        "app.Cycle".into(),
        NominalTypeTemplate {
            params: vec![],
            body: CoreType::Named("Cycle".into()),
        },
    );
    assert_eq!(
        templates.field_type(&CoreType::Named("app.Execution".into()), "sequence", "app"),
        Some(CoreType::Int)
    );
    for ty in [
        CoreType::Int,
        CoreType::Named("Missing".into()),
        CoreType::Named("Cycle".into()),
        CoreType::Apply {
            constructor: "app.Execution".into(),
            args: vec![CoreType::Int],
        },
    ] {
        assert_eq!(templates.field_type(&ty, "sequence", "app"), None);
    }
}

#[test]
fn structural_field_projection_covers_records_products_and_maps() {
    let templates = CallableTemplates::default();
    let product = CoreType::Tuple(vec![
        CoreTupleTypeElem::Type(CoreType::String),
        CoreTupleTypeElem::Field {
            name: "count".into(),
            ty: CoreType::Int,
        },
    ]);
    let record = CoreType::Struct {
        name: "app.Record".into(),
        fields: vec![field("enabled", CoreType::Bool)],
    };
    let map = CoreType::Map(vec![CoreMapTypeField {
        key: "label".into(),
        operator: ":".into(),
        value: CoreType::String,
    }]);
    for (ty, name, expected) in [
        (product, "count", Some(CoreType::Int)),
        (record, "enabled", Some(CoreType::Bool)),
        (map, "label", Some(CoreType::String)),
    ] {
        assert_eq!(templates.field_type(&ty, name, "app"), expected);
        assert_eq!(templates.field_type(&ty, "missing", "app"), None);
    }
}
