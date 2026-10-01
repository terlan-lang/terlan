use super::*;

fn text(value: &str) -> NativeValue {
    value.into()
}

#[test]
fn bindings_preserve_codec_output_and_optional_attributes() {
    let simple = [
        text("sid"),
        text("abc"),
        text("/"),
        text(""),
        NativeValue::Int(0),
        NativeValue::Bool(false),
        text(""),
        NativeValue::Bool(true),
        NativeValue::Bool(false),
        text(""),
    ];
    assert_eq!(
        (SET_HEADER_WITH_OPTIONS.invoke)(&simple).unwrap(),
        text("sid=abc; HttpOnly; Path=/")
    );
    assert_eq!(
        (SET_HEADER_WITH_OPTIONS.invoke)(&[
            text("sid"),
            text(""),
            text("/"),
            text(""),
            NativeValue::Int(0),
            NativeValue::Bool(true),
            text("Thu, 01 Jan 1970 00:00:00 GMT"),
            NativeValue::Bool(false),
            NativeValue::Bool(false),
            text("")
        ])
        .unwrap(),
        text("sid=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT")
    );
    for (policy, expected) in [
        ("", ""),
        ("LaX", "; SameSite=Lax"),
        ("strict", "; SameSite=Strict"),
        ("NONE", "; SameSite=None; Secure"),
    ] {
        let args = [
            text("sid"),
            text("abc"),
            text("/"),
            text(""),
            NativeValue::Int(42),
            NativeValue::Bool(false),
            text(""),
            NativeValue::Bool(false),
            NativeValue::Bool(false),
            text(policy),
        ];
        assert_eq!(
            (SET_HEADER_WITH_OPTIONS.invoke)(&args).unwrap(),
            text(&format!("sid=abc{expected}; Path=/"))
        );
    }
    let args = [
        text("sid"),
        text("abc"),
        text("/account"),
        text("example.com"),
        NativeValue::Int(3600),
        NativeValue::Bool(true),
        text("Wed, 21 Oct 2015 07:28:00 GMT"),
        NativeValue::Bool(true),
        NativeValue::Bool(true),
        text("strict"),
    ];
    assert_eq!((SET_HEADER_WITH_OPTIONS.invoke)(&args).unwrap(), text("sid=abc; HttpOnly; SameSite=Strict; Secure; Path=/account; Domain=example.com; Max-Age=3600; Expires=Wed, 21 Oct 2015 07:28:00 GMT"));
}

#[test]
fn every_argument_is_checked_and_injection_is_rejected() {
    let binding = &SET_HEADER_WITH_OPTIONS;
    let args = vec![
        text("sid"),
        text("abc"),
        text("/"),
        text(""),
        NativeValue::Int(0),
        NativeValue::Bool(false),
        text(""),
        NativeValue::Bool(false),
        NativeValue::Bool(false),
        text(""),
    ];
    assert_eq!(binding.arity, args.len());
    assert!((binding.invoke)(&[]).is_err());
    let mut extra = args.clone();
    extra.push(NativeValue::Unit);
    assert!((binding.invoke)(&extra).is_err());
    for index in 0..args.len() {
        let mut wrong = args.clone();
        wrong[index] = NativeValue::Unit;
        assert_eq!(
            (binding.invoke)(&wrong).unwrap_err().code(),
            "dispatch.type"
        );
    }
    for bad in ["", "$reserved", "x\r\nInjected", "a;b", "a b", "é"] {
        let mut wrong = args.clone();
        wrong[0] = text(bad);
        assert_eq!(
            (binding.invoke)(&wrong).unwrap_err().code(),
            "http.cookie.invalid_name"
        );
    }
    let mut args = args;
    for (index, bad, code) in [
        (1, "a;b", "http.cookie.invalid_value"),
        (2, "relative", "http.cookie.invalid_path"),
        (3, "x\r\nInjected", "http.cookie.invalid_attribute"),
        (6, "not-a-date", "http.cookie.invalid_attribute"),
        (9, "unknown", "dispatch.http.cookie.invalid_same_site"),
    ] {
        let old = std::mem::replace(&mut args[index], text(bad));
        assert_eq!(
            (SET_HEADER_WITH_OPTIONS.invoke)(&args).unwrap_err().code(),
            code
        );
        args[index] = old;
    }
}
