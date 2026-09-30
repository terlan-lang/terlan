use crate::NativeValue;
use std::cell::Cell;

struct Upstream {
    text: String,
    calls: Cell<usize>,
}

impl Upstream {
    fn as_str(&self) -> &str {
        self.calls.set(self.calls.get() + 1);
        &self.text
    }

    fn optional(&self) -> Option<&str> {
        Some("")
    }

    fn absent(&self) -> Option<&str> {
        None
    }

    fn result(&self) -> Result<&str, &str> {
        Err(&self.text)
    }
}

#[test]
fn accessor_projection_preserves_names_order_optional_values_and_ownership() {
    let record = {
        let value = Upstream {
            text: "\u{754c}\0text".into(),
            calls: Cell::new(0),
        };
        let record = native_record!(ApplicationValue, value, { as_str, optional, absent, result });
        assert_eq!(value.calls.get(), 1);
        record
    };
    assert_eq!(
        record,
        NativeValue::Record {
            name: "ApplicationValue".into(),
            fields: vec![
                ("as_str".into(), "\u{754c}\0text".into()),
                ("optional".into(), Some("").into()),
                ("absent".into(), None::<&str>.into()),
                ("result".into(), Err::<&str, _>("\u{754c}\0text").into()),
            ],
        }
    );
}

#[test]
fn projection_evaluates_temporary_receiver_once_without_capturing_caller_names() {
    let calls = Cell::new(0);
    let as_str = "caller binding";
    let __native_record_source = "caller source";
    let record = native_record!(
        Temporary,
        {
            calls.set(calls.get() + 1);
            String::from("temporary")
        },
        { as_str }
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(as_str, "caller binding");
    assert_eq!(__native_record_source, "caller source");
    assert_eq!(
        record,
        NativeValue::Record {
            name: "Temporary".into(),
            fields: vec![("as_str".into(), "temporary".into())],
        }
    );
}
