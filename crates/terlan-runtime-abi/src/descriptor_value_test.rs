use super::*;
use crate::NativeValue as V;

#[test]
fn owned_descriptors_transfer_storage_and_keep_resources_opaque() {
    let text = String::from("owned text");
    let pointer = text.as_ptr();
    let D::String(text) = V::String(text).into_descriptor() else {
        panic!("text");
    };
    assert_eq!(text.as_ptr(), pointer);
    assert!(matches!(V::Int(42).into_descriptor(), D::Int(42)));
    assert!(matches!(V::Atom("tag".into()).into_descriptor(), D::Atom(tag) if tag == "tag"));
    let entries = vec![V::Int(1)];
    let pointer = entries.as_ptr();
    let D::List(entries) = V::List(entries).into_descriptor() else {
        panic!("list");
    };
    assert_eq!(entries.as_ptr(), pointer);
    assert!(
        matches!(V::Tuple(entries).into_descriptor(), D::Tuple(values) if values == [V::Int(1)])
    );
    let fields = vec![("field".into(), V::Int(7))];
    let pointer = fields.as_ptr();
    let D::Record(name, fields) = (V::Record {
        name: "Data".into(),
        fields,
    })
    .into_descriptor() else {
        panic!("record");
    };
    assert_eq!(name, "Data");
    assert_eq!(fields.as_ptr(), pointer);
    assert!(matches!(V::Unit.into_descriptor(), D::Unit));
    for opaque in [
        V::Bool(false),
        V::Bytes(vec![255]),
        V::Float(1.0),
        V::Map(vec![]),
    ] {
        assert!(matches!(opaque.into_descriptor(), D::Opaque));
    }
}

use OwnedDescriptor as D;
