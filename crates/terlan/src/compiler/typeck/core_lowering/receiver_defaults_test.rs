use super::*;

#[test]
fn adapters_keep_checked_signatures_but_do_not_claim_native_exports() {
    let mut module = crate::terlan_syntax::parse_module_as_syntax_output(
        r#"
module receiver_default_adapters.
pub opaque type Handle.
@compiler.native {example.adjust}
pub (mut handle: Handle) adjust(amount: Int, step: Int = 2, enabled: Bool = true): Unit -> native.
pub (handle: Handle) identity(): Handle -> handle.
pub plain(value: Int = 1): Int -> value.
"#,
    )
    .unwrap();
    let original = module.declarations.len();
    materialize(&mut module);
    assert_eq!(module.declarations.len(), original + 2);
    for (index, declaration) in module.declarations[original..].iter().enumerate() {
        assert!(declaration.annotations.is_empty());
        let SyntaxDeclarationPayload::Method {
            receiver,
            name,
            params,
            clauses,
            return_type,
            is_public,
            ..
        } = &declaration.payload
        else {
            panic!("ordinary receiver adapter")
        };
        assert!(receiver.is_mutable);
        assert!(*is_public);
        assert_eq!(name, "adjust");
        assert_eq!(return_type.text, "Unit");
        assert_eq!(params.len(), index + 1);
        assert!(params
            .iter()
            .all(|param| !param.has_default && param.default.is_none()));
        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Call);
        assert_eq!(body.arity, 4);
        assert_eq!(body.children[0].text.as_deref(), Some("adjust"));
        assert_eq!(body.children[1].text.as_deref(), Some("handle"));
        assert_eq!(body.children[2].text.as_deref(), Some("amount"));
        assert_eq!(
            body.children[3].text.as_deref(),
            Some(if index == 0 { "2" } else { "step" })
        );
        assert_eq!(body.children[4].text.as_deref(), Some("true"));
    }
}
