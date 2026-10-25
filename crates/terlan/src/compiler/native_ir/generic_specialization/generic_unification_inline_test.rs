use super::*;

#[test]
fn repeated_generic_witness_accepts_recursive_nominal_and_struct_views() {
    let name = "app.Node".to_string();
    let nominal = CoreType::Named(name.clone());
    let structural = CoreType::Struct {
        name,
        fields: vec![CoreStructTypeField {
            name: "children".to_string(),
            ty: CoreType::List(Box::new(nominal.clone())),
            is_private: false,
        }],
    };
    let mut substitution = HashMap::new();
    unify(
        &CoreType::Named("T".to_string()),
        &nominal,
        &["T".to_string()],
        &mut substitution,
    )
    .unwrap();
    unify(
        &CoreType::Named("T".to_string()),
        &structural,
        &["T".to_string()],
        &mut substitution,
    )
    .unwrap();
    assert_eq!(substitution["T"], nominal);
}
