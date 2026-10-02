use super::*;
use serde_json::{json, Value};

fn validate(value: Value) -> Result<Config, String> {
    Ok(serde_json::from_value::<Settings>(value)
        .map_err(|error| error.to_string())?
        .validate()?)
}

#[test]
fn defaults_and_valid_modes_preserve_exact_settings_without_io() {
    assert_eq!(
        Settings::default().validate().unwrap_err().to_string(),
        "project manifest [server.tls] requires mode"
    );
    let internal = validate(json!({"mode": "internal"})).unwrap();
    assert_eq!(internal.mode, Mode::Internal);
    assert!(internal.domains.is_empty());
    assert_eq!(internal.trust_local, None);
    let auto = validate(json!({
        "mode": "auto", "domains": ["example.test", "other.test"],
        "email": " admin@example.test ", "primary_provider": "letsencrypt",
        "fallback_provider": "zerossl"
    }))
    .unwrap();
    assert_eq!(auto.domains, ["example.test", "other.test"]);
    assert_eq!(auto.email.as_deref(), Some(" admin@example.test "));
    assert_eq!(auto.primary_provider, Some(Provider::LetsEncrypt));
    assert_eq!(auto.fallback_provider, Some(Provider::ZeroSsl));
    let manual = validate(json!({
        "mode": "manual", "domains": ["local"], "email": "user@example.test",
        "cert": "/missing/cert.pem", "key": "/missing/key.pem", "ca": "/missing/ca.pem",
        "passphrase_env": "MISSING_ENV", "server_name": "local", "trust_local": false
    }))
    .unwrap();
    assert_eq!(manual.cert.as_deref(), Some("/missing/cert.pem"));
    assert_eq!(manual.key.as_deref(), Some("/missing/key.pem"));
    assert_eq!(manual.ca.as_deref(), Some("/missing/ca.pem"));
    assert_eq!(manual.passphrase_env.as_deref(), Some("MISSING_ENV"));
    assert_eq!(manual.server_name.as_deref(), Some("local"));
    assert_eq!(manual.trust_local, Some(false));
    assert_eq!(
        validate(json!({"mode": "internal", "trust_local": true}))
            .unwrap()
            .trust_local,
        Some(true)
    );
}

#[test]
fn all_string_fields_reject_blank_values_without_normalizing_nonblank_text() {
    for field in [
        "email",
        "cert",
        "key",
        "passphrase_env",
        "ca",
        "server_name",
    ] {
        for value in ["", " ", "\t\r\n", "\u{2003}"] {
            let mut fields = json!({"mode": "manual", "cert": "cert", "key": "key"});
            fields[field] = json!(value);
            assert_eq!(
                validate(fields).unwrap_err(),
                format!("project manifest [server.tls] {field} cannot be empty")
            );
        }
    }
    for domains in [
        json!([""]),
        json!(["valid", " \t"]),
        json!(["\u{2003}", "valid"]),
    ] {
        assert_eq!(
            validate(json!({"mode": "auto", "domains": domains})).unwrap_err(),
            "project manifest [server.tls] domains cannot contain empty entries"
        );
    }
}

#[test]
fn each_forbidden_field_is_checked_independently_in_auto_and_internal_modes() {
    for field in [
        "cert",
        "key",
        "passphrase_env",
        "ca",
        "server_name",
        "trust_local",
    ] {
        let mut fields = json!({"mode": "auto", "domains": ["example.test"]});
        fields[field] = if field == "trust_local" {
            json!(false)
        } else {
            json!("present")
        };
        assert_eq!(
            validate(fields).unwrap_err(),
            "project manifest [server.tls] mode auto cannot set manual or internal TLS fields"
        );
    }
    for field in [
        "domains",
        "email",
        "primary_provider",
        "fallback_provider",
        "cert",
        "key",
        "passphrase_env",
        "ca",
    ] {
        let mut fields = json!({"mode": "internal"});
        fields[field] = match field {
            "domains" => json!([]),
            "primary_provider" | "fallback_provider" => json!("letsencrypt"),
            _ => json!("present"),
        };
        assert_eq!(
            validate(fields).unwrap_err(),
            "project manifest [server.tls] mode internal cannot set public or manual TLS fields"
        );
    }
}

#[test]
fn required_fields_and_manual_provider_exclusions_are_enforced() {
    for fields in [
        json!({"mode": "auto"}),
        json!({"mode": "auto", "domains": []}),
    ] {
        assert_eq!(
            validate(fields).unwrap_err(),
            "project manifest [server.tls] mode auto requires domains"
        );
    }
    for fields in [
        json!({"mode": "manual"}),
        json!({"mode": "manual", "cert": "cert"}),
        json!({"mode": "manual", "key": "key"}),
    ] {
        assert_eq!(
            validate(fields).unwrap_err(),
            "project manifest [server.tls] mode manual requires cert and key"
        );
    }
    for field in ["primary_provider", "fallback_provider"] {
        let mut fields = json!({"mode": "manual", "cert": "cert", "key": "key"});
        fields[field] = json!("zerossl");
        assert_eq!(
            validate(fields).unwrap_err(),
            "project manifest [server.tls] mode manual cannot set ACME providers"
        );
    }
}

#[test]
fn enum_parsing_has_one_exact_contract_for_incremental_and_serde_paths() {
    for (value, expected) in [
        ("auto", Mode::Auto),
        ("manual", Mode::Manual),
        ("internal", Mode::Internal),
    ] {
        assert_eq!(value.parse::<Mode>().unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<Mode>(json!(value)).unwrap(),
            expected
        );
    }
    for (value, expected) in [
        ("letsencrypt", Provider::LetsEncrypt),
        ("zerossl", Provider::ZeroSsl),
    ] {
        assert_eq!(value.parse::<Provider>().unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<Provider>(json!(value)).unwrap(),
            expected
        );
    }
    for value in [
        "",
        "Auto",
        "auto ",
        " manual",
        "unknown",
        "auto\0",
        "LetsEncrypt",
    ] {
        let mode_error = value.parse::<Mode>().unwrap_err();
        assert!(mode_error.contains("supported modes: auto, manual, internal"));
        assert!(serde_json::from_value::<Mode>(json!(value))
            .unwrap_err()
            .to_string()
            .contains(&mode_error));
        let provider_error = value.parse::<Provider>().unwrap_err();
        assert!(provider_error.contains("supported providers: letsencrypt, zerossl"));
        assert!(serde_json::from_value::<Provider>(json!(value))
            .unwrap_err()
            .to_string()
            .contains(&provider_error));
    }
}

#[test]
fn deserialization_rejects_unknown_fields_wrong_shapes_and_duplicate_keys() {
    for fields in [
        json!({"mode": false}),
        json!({"mode": "auto", "domains": "host"}),
        json!({"mode": "internal", "trust_local": "true"}),
        json!({"mode": "internal", "extra": 1}),
        json!({"mode": "auto", "domains": [false]}),
        json!({"mode": "internal", "server_name": 123}),
    ] {
        assert!(serde_json::from_value::<Settings>(fields).is_err());
    }
    assert!(serde_json::from_str::<Settings>(r#"{"mode":"auto","mode":"internal"}"#).is_err());
}
