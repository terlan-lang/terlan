use super::*;

#[test]
fn consuming_descriptor_preserves_vm_allocations_and_generic_shapes() {
    let text = String::from("owned body");
    let pointer = text.as_ptr();
    let OwnedDescriptor::String(text) = ReplValue::String(text).into_descriptor() else {
        panic!("text");
    };
    assert_eq!(text.as_ptr(), pointer);
    assert!(matches!(
        ReplValue::Int(7).into_descriptor(),
        OwnedDescriptor::Int(7)
    ));
    assert!(
        matches!(ReplValue::Atom("tag".into()).into_descriptor(), OwnedDescriptor::Atom(tag) if tag == "tag")
    );
    let entries = vec![ReplValue::Int(1)];
    let pointer = entries.as_ptr();
    let OwnedDescriptor::List(entries) = ReplValue::List(entries).into_descriptor() else {
        panic!("list");
    };
    assert_eq!(entries.as_ptr(), pointer);
    assert!(
        matches!(ReplValue::Tuple(entries).into_descriptor(), OwnedDescriptor::Tuple(values) if values == [ReplValue::Int(1)])
    );
    let fields = vec![("field".into(), ReplValue::Int(1))];
    let pointer = fields.as_ptr();
    let OwnedDescriptor::Record(name, fields) = (ReplValue::Record {
        name: "Data".into(),
        fields,
    })
    .into_descriptor() else {
        panic!("record");
    };
    assert_eq!(name, "Data");
    assert_eq!(fields.as_ptr(), pointer);
    assert!(matches!(
        ReplValue::Unit.into_descriptor(),
        OwnedDescriptor::Opaque
    ));
}

#[test]
fn byte_backed_text_requires_valid_utf8_for_borrowed_and_owned_views() {
    let text = ReplValue::StringBytes(bytes::Bytes::from_static(b"body"));
    assert!(matches!(
        text.descriptor_view(),
        DescriptorView::String("body")
    ));
    assert!(matches!(text.into_descriptor(), OwnedDescriptor::String(value) if value == "body"));
    let invalid = ReplValue::StringBytes(bytes::Bytes::from_static(b"\xff"));
    assert!(matches!(invalid.descriptor_view(), DescriptorView::Opaque));
    assert!(matches!(invalid.into_descriptor(), OwnedDescriptor::Opaque));
}
