//! Source schemas, not domain names, establish scalar specialization safety.

use super::super::*;
use crate::runtime::native_image::managed::encode_string_literal;
use std::sync::Arc;

#[test]
fn ordinary_source_record_specialization_executes_as_native_code() {
    let mut modules = source_constructor_test::check_sources(&[r#"
module projection_source.
pub struct Document { title: String, version: Int }.
pub ready(value: Document): Bool -> value.title == "ready".
pub check(): Bool -> ready(Document { title: "ready", version: 7 }).
"#]);
    let proofs = install_native_aggregate_projection_exports(&mut modules);
    let proof = proofs
        .iter()
        .find(|proof| proof.function == "ready")
        .unwrap();
    assert_eq!(proof.scalar_field, Some(0));
    let entry = proof
        .scalar_entry
        .as_ref()
        .expect("source layout proves String");
    assert_eq!(modules.len(), 1);
    let index = modules[0]
        .functions
        .iter()
        .position(|function| &function.name == entry)
        .unwrap();
    let check = modules[0]
        .functions
        .iter_mut()
        .find(|function| function.name == "check")
        .unwrap();
    let export_id = check.export_id;
    check.body = NativeExpr::Call {
        function: index,
        args: vec![NativeExpr::ManagedLiteral {
            encoded: Arc::from(encode_string_literal("ready").unwrap()),
        }],
    };
    let object = emit_native_application_object("projection_source", &modules).unwrap();
    native_object_test_support::assert_managed_native_object_invocations(
        "projection_source",
        &modules,
        &object,
        &[native_object_test_support::NativeObjectInvocation {
            export_id,
            arguments: vec![],
            expected_status: status::OK,
            expected_result: Some(1),
        }],
    );
}
