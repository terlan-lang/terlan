//! Cloud identity, credential, and response validation.

use super::*;

pub(super) fn read_token(path: &Path, purpose: &str) -> Result<String, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot inspect {purpose} token {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "error[cloud_token]: {purpose} token path is not a regular file"
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(format!(
                "error[cloud_token_permissions]: {purpose} token file must not be accessible by group or other users"
            ));
        }
    }
    let token = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {purpose} token {}: {error}", path.display()))?;
    let token = token.trim().to_string();
    validate_token(&token, purpose)?;
    Ok(token)
}

pub(super) fn validate_token(token: &str, purpose: &str) -> Result<(), String> {
    if token.len() < 24 || token.len() > 512 || token.chars().any(char::is_whitespace) {
        return Err(format!("error[cloud_token]: {purpose} token is malformed"));
    }
    Ok(())
}

pub(super) fn validate_user_id(value: &str) -> Result<(), String> {
    validate_prefixed_identity(value, "usr_", "user")
}

pub(super) fn validate_release_id(value: &str) -> Result<(), String> {
    validate_prefixed_identity(value, "rel_", "release")
}

pub(super) fn validate_deployment_id(value: &str) -> Result<(), String> {
    validate_prefixed_identity(value, "dep_", "deployment")
}

pub(super) fn validate_prefixed_identity(
    value: &str,
    prefix: &str,
    purpose: &str,
) -> Result<(), String> {
    let suffix_length = value.len().saturating_sub(prefix.len());
    if !(16..=64).contains(&suffix_length)
        || !value.starts_with(prefix)
        || !value[prefix.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return Err(format!("error[cloud_identity]: invalid {purpose} identity"));
    }
    Ok(())
}

pub(super) fn validate_project_slug(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 63
        || value.starts_with('-')
        || value.ends_with('-')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("error[cloud_project]: invalid project slug".into());
    }
    Ok(())
}

pub(super) fn validate_repository(value: &str) -> Result<(), String> {
    let url = Url::parse(value)
        .map_err(|_| "error[cloud_source_repository]: repository URL is invalid".to_string())?;
    if url.scheme() != "https"
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || value.len() > 512
    {
        return Err("error[cloud_source_repository]: repository must use HTTPS".into());
    }
    Ok(())
}

pub(super) fn validate_release_version(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'+' | b'.'))
    {
        return Err("error[cloud_release_version]: release version is invalid".into());
    }
    Ok(())
}

pub(super) fn validate_routing_host(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 253
        || value.contains(['/', ':', '\r', '\n', ' '])
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
    {
        return Err("error[cloud_routing_host]: routing host is invalid".into());
    }
    Ok(())
}

pub(super) fn validate_bounded_value(
    purpose: &str,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), String> {
    if value.len() < minimum
        || value.len() > maximum
        || value.chars().any(|character| character.is_control())
    {
        return Err(format!("error[cloud_value]: invalid {purpose}"));
    }
    Ok(())
}

pub(super) fn random_id(prefix: &str) -> String {
    format!("{prefix}{}", random_hex())
}

pub(super) fn random_hex() -> String {
    let mut bytes = [0_u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) fn read_json(path: &Path, purpose: &str) -> Result<Value, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot read {purpose} {}: {error}", path.display()))?;
    parse_json(&bytes, purpose)
}

pub(super) fn parse_json(bytes: &[u8], purpose: &str) -> Result<Value, String> {
    serde_json::from_slice(bytes)
        .map_err(|error| format!("error[cloud_json]: {purpose} is invalid: {error}"))
}

pub(super) fn require_schema(value: &Value, schema: &str, purpose: &str) -> Result<(), String> {
    if value.get("schema").and_then(Value::as_str) != Some(schema) {
        return Err(format!("error[cloud_schema]: {purpose} schema is invalid"));
    }
    Ok(())
}

pub(super) fn required_json_string(
    value: &Value,
    field: &str,
    purpose: &str,
) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("error[cloud_json]: {purpose} field `{field}` is missing"))
}

pub(super) fn print_json(value: &Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value)
            .map_err(|error| format!("cannot render Cloud response: {error}"))?
    );
    Ok(())
}
