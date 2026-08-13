use super::*;

#[test]
fn parses_native_block_signatures_and_types() {
    let source = "native core module VecNative {\n    #[native(normal)]\n    empty[T](): Vec[T].\n\n    #[native(normal)]\n    push[T](V: Vec[T], Item: T): Vec[T].\n}";

    assert_eq!(
        extract_native_module_name(source).as_deref(),
        Some("VecNative")
    );
    assert_eq!(extract_native_scheduler(source).as_deref(), Some("normal"));

    let signatures = extract_native_function_signatures(source);
    assert_eq!(signatures.len(), 2);

    assert_eq!(signatures[0].name, "empty");
    assert_eq!(signatures[0].arity, 0);
    assert_eq!(signatures[0].params.len(), 0);
    assert_eq!(signatures[0].return_type, "Vec[T]");

    assert_eq!(signatures[1].name, "push");
    assert_eq!(signatures[1].arity, 2);
    assert_eq!(
        signatures[1].params[0],
        ("V".to_string(), "Vec[T]".to_string())
    );
    assert_eq!(
        signatures[1].params[1],
        ("Item".to_string(), "T".to_string())
    );
    assert_eq!(signatures[1].return_type, "Vec[T]");
}

#[test]
fn parses_native_signatures_from_block_without_newlines() {
    let source = "native core module VecNative { #[native(normal)] empty[T](): Vec[T]. #[native(normal)] push[T](V: Vec[T], Item: T): Vec[T]. }";

    let signatures = extract_native_function_signatures(source);
    assert_eq!(signatures.len(), 2);
    assert_eq!(signatures[0].name, "empty");
    assert_eq!(signatures[1].name, "push");
    assert_eq!(signatures[1].arity, 2);
    assert_eq!(signatures[1].return_type, "Vec[T]");
}

#[test]
fn quoted_defaults_do_not_change_native_signature_boundaries() {
    let signature = parse_native_function_signature(
        r#"csv(separator: String = ",", quote: String = "\"", suffix: String = "a)b"): String"#,
    )
    .expect("native signature");

    assert_eq!(signature.name, "csv");
    assert_eq!(signature.arity, 3);
    assert_eq!(signature.params.len(), 3);
    assert_eq!(signature.params[0].0, "separator");
    assert_eq!(signature.params[1].0, "quote");
    assert_eq!(signature.params[2].0, "suffix");
    assert_eq!(signature.return_type, "String");
}

#[test]
fn trailing_comma_does_not_create_a_phantom_native_parameter() {
    assert_eq!(
        native_signature_arity("first: String, second: List[Int],"),
        2
    );
    assert_eq!(native_signature_arity("only: Bool,"), 1);
}
