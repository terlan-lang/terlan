use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_layout, encode_string_literal, ManagedAggregateDescriptor, ManagedFieldType,
};

use super::NativeExpr;

#[test]
fn managed_encoding_inventory_reaches_every_nested_expression_position() {
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::tuple(
            "{Option[String], Option[String], Option[String], Option[String]}",
            vec![
                ManagedFieldType::Reference(
                    crate::runtime::native_image::managed::managed_string_semantic_id(),
                );
                4
            ],
        )
        .expect("tuple descriptor"),
    );
    let tuple =
        Arc::<[u8]>::from(encode_aggregate_layout(&descriptor).expect("encode tuple descriptor"));
    let string = Arc::<[u8]>::from(encode_string_literal("value").expect("encode string"));
    let expression = NativeExpr::Let {
        bindings: vec![NativeExpr::Construct {
            descriptor,
            encoded_layout: tuple.clone(),
            fields: vec![NativeExpr::ManagedLiteral {
                encoded: string.clone(),
            }],
        }],
        body: Box::new(NativeExpr::If {
            clauses: vec![(
                NativeExpr::Bool(true),
                NativeExpr::ManagedOperation {
                    encoded: string.clone(),
                    args: vec![NativeExpr::MakeClosure {
                        encoded: string.clone(),
                        captures: vec![NativeExpr::ManagedLiteral {
                            encoded: string.clone(),
                        }],
                    }],
                },
            )],
        }),
    };

    let mut encodings = Vec::new();
    expression.collect_managed_encodings(&mut encodings);

    assert_eq!(
        encodings,
        [
            tuple,
            string.clone(),
            string.clone(),
            string.clone(),
            string
        ]
    );
}
