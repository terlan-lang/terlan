use super::*;

fn arguments() -> Vec<NativeValue> {
    vec![
        "sid".into(),
        "abc".into(),
        "/".into(),
        None::<String>.into(),
        None::<i64>.into(),
        None::<String>.into(),
        false.into(),
        false.into(),
        None::<String>.into(),
    ]
}

#[test]
fn bindings_preserve_codec_output_and_optional_attributes() {
    let mut args = arguments();
    assert_eq!(
        (ENCODE_COOKIE.invoke)(&args).unwrap(),
        "sid=abc; Path=/".into()
    );
    for (policy, expected) in [
        ("lax", "; SameSite=Lax"),
        ("strict", "; SameSite=Strict"),
        ("none", "; SameSite=None; Secure"),
    ] {
        args[8] = Some(policy).into();
        assert_eq!(
            (ENCODE_COOKIE.invoke)(&args).unwrap(),
            format!("sid=abc{expected}; Path=/").into()
        );
    }
    args[2] = "/account".into();
    args[3] = Some("example.com").into();
    args[4] = Some(3600_i64).into();
    args[5] = Some("Wed, 21 Oct 2015 07:28:00 GMT").into();
    args[6] = true.into();
    args[7] = true.into();
    args[8] = Some("strict").into();
    assert_eq!((ENCODE_COOKIE.invoke)(&args).unwrap(), NativeValue::from("sid=abc; HttpOnly; SameSite=Strict; Secure; Path=/account; Domain=example.com; Max-Age=3600; Expires=Wed, 21 Oct 2015 07:28:00 GMT"));

    let mut args = arguments();
    args[1] = "".into();
    args[4] = Some(0_i64).into();
    args[5] = Some("Thu, 01 Jan 1970 00:00:00 GMT").into();
    assert_eq!(
        (ENCODE_COOKIE.invoke)(&args).unwrap(),
        NativeValue::from("sid=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT")
    );
}

#[test]
fn every_argument_is_checked_and_injection_is_rejected() {
    let args = arguments();
    assert_eq!(ENCODE_COOKIE.arity, args.len());
    for count in 0..args.len() {
        assert!((ENCODE_COOKIE.invoke)(&args[..count]).is_err());
    }
    let mut extra = args.clone();
    extra.push(NativeValue::Unit);
    assert!((ENCODE_COOKIE.invoke)(&extra).is_err());
    for index in 0..args.len() {
        let mut wrong = args.clone();
        wrong[index] = NativeValue::Unit;
        assert_eq!(
            (ENCODE_COOKIE.invoke)(&wrong).unwrap_err().code(),
            "dispatch.type"
        );
    }
    for index in [3, 4, 5, 8] {
        for malformed in [
            Some(true).into(),
            NativeValue::Record {
                name: "Some".into(),
                fields: vec![],
            },
            NativeValue::Record {
                name: "None".into(),
                fields: vec![("value".into(), "".into())],
            },
            NativeValue::Record {
                name: "Some".into(),
                fields: vec![("wrong".into(), "".into())],
            },
        ] {
            let mut wrong = args.clone();
            wrong[index] = malformed;
            assert_eq!(
                (ENCODE_COOKIE.invoke)(&wrong).unwrap_err().code(),
                "dispatch.type"
            );
        }
    }
    for bad in ["", "$reserved", "x\r\nInjected", "a;b", "a b", "é"] {
        let mut wrong = args.clone();
        wrong[0] = bad.into();
        assert_eq!(
            (ENCODE_COOKIE.invoke)(&wrong).unwrap_err().code(),
            "http.cookie.invalid_name"
        );
    }
    for (index, bad, code) in [
        (1, NativeValue::from("a;b"), "http.cookie.invalid_value"),
        (2, "relative".into(), "http.cookie.invalid_path"),
        (
            3,
            Some("x\r\nInjected").into(),
            "http.cookie.invalid_attribute",
        ),
        (3, Some("").into(), "http.cookie.invalid_attribute"),
        (
            5,
            Some("not-a-date").into(),
            "http.cookie.invalid_attribute",
        ),
        (5, Some("").into(), "http.cookie.invalid_attribute"),
        (
            8,
            Some("unknown").into(),
            "dispatch.http.cookie.invalid_same_site",
        ),
        (8, Some("").into(), "dispatch.http.cookie.invalid_same_site"),
        (
            8,
            Some("LaX").into(),
            "dispatch.http.cookie.invalid_same_site",
        ),
    ] {
        let mut wrong = args.clone();
        wrong[index] = bad;
        assert_eq!((ENCODE_COOKIE.invoke)(&wrong).unwrap_err().code(), code);
    }
}
