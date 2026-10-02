//! Declarative TLS admission shared by build tooling and serving hosts.
//! No filesystem, environment, certificate, scheduler, or network access.

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(try_from = "String")]
pub enum Mode {
    Auto,
    Manual,
    Internal,
}

impl FromStr for Mode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "manual" => Ok(Self::Manual),
            "internal" => Ok(Self::Internal),
            _ => Err(format!(
                "unsupported [server.tls] mode `{value}`; supported modes: auto, manual, internal"
            )),
        }
    }
}

impl TryFrom<String> for Mode {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(try_from = "String")]
pub enum Provider {
    LetsEncrypt,
    ZeroSsl,
}

impl FromStr for Provider {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "letsencrypt" => Ok(Self::LetsEncrypt),
            "zerossl" => Ok(Self::ZeroSsl),
            _ => Err(format!(
                "unsupported [server.tls] provider `{value}`; supported providers: letsencrypt, zerossl"
            )),
        }
    }
}

impl TryFrom<String> for Provider {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub mode: Mode,
    pub domains: Vec<String>,
    pub email: Option<String>,
    pub primary_provider: Option<Provider>,
    pub fallback_provider: Option<Provider>,
    pub cert: Option<String>,
    pub key: Option<String>,
    pub passphrase_env: Option<String>,
    pub ca: Option<String>,
    pub server_name: Option<String>,
    pub trust_local: Option<bool>,
}

/// Presence is significant: even false or an empty list is an explicit setting.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub mode: Option<Mode>,
    pub domains: Option<Vec<String>>,
    pub email: Option<String>,
    pub primary_provider: Option<Provider>,
    pub fallback_provider: Option<Provider>,
    pub cert: Option<String>,
    pub key: Option<String>,
    pub passphrase_env: Option<String>,
    pub ca: Option<String>,
    pub server_name: Option<String>,
    pub trust_local: Option<bool>,
}

impl Settings {
    /// Validate an explicitly present section without materializing TLS resources.
    pub fn validate(self) -> Result<Config, crate::ServiceError> {
        for (field, value) in [
            ("email", self.email.as_deref()),
            ("cert", self.cert.as_deref()),
            ("key", self.key.as_deref()),
            ("passphrase_env", self.passphrase_env.as_deref()),
            ("ca", self.ca.as_deref()),
            ("server_name", self.server_name.as_deref()),
        ] {
            if value.is_some_and(|value| value.trim().is_empty()) {
                return Err(
                    format!("project manifest [server.tls] {field} cannot be empty").into(),
                );
            }
        }
        if self
            .domains
            .as_ref()
            .is_some_and(|domains| domains.iter().any(|domain| domain.trim().is_empty()))
        {
            return Err(
                "project manifest [server.tls] domains cannot contain empty entries".into(),
            );
        }
        let mode = self
            .mode
            .ok_or("project manifest [server.tls] requires mode")?;
        match mode {
            Mode::Auto => {
                if self.domains.as_ref().is_none_or(Vec::is_empty) {
                    return Err("project manifest [server.tls] mode auto requires domains".into());
                }
                if self.cert.is_some()
                    || self.key.is_some()
                    || self.passphrase_env.is_some()
                    || self.ca.is_some()
                    || self.server_name.is_some()
                    || self.trust_local.is_some()
                {
                    return Err("project manifest [server.tls] mode auto cannot set manual or internal TLS fields".into());
                }
            }
            Mode::Manual => {
                if self.cert.is_none() || self.key.is_none() {
                    return Err(
                        "project manifest [server.tls] mode manual requires cert and key".into(),
                    );
                }
                if self.primary_provider.is_some() || self.fallback_provider.is_some() {
                    return Err(
                        "project manifest [server.tls] mode manual cannot set ACME providers"
                            .into(),
                    );
                }
            }
            Mode::Internal => {
                if self.domains.is_some()
                    || self.email.is_some()
                    || self.primary_provider.is_some()
                    || self.fallback_provider.is_some()
                    || self.cert.is_some()
                    || self.key.is_some()
                    || self.passphrase_env.is_some()
                    || self.ca.is_some()
                {
                    return Err("project manifest [server.tls] mode internal cannot set public or manual TLS fields".into());
                }
            }
        }
        Ok(Config {
            mode,
            domains: self.domains.unwrap_or_default(),
            email: self.email,
            primary_provider: self.primary_provider,
            fallback_provider: self.fallback_provider,
            cert: self.cert,
            key: self.key,
            passphrase_env: self.passphrase_env,
            ca: self.ca,
            server_name: self.server_name,
            trust_local: self.trust_local,
        })
    }
}

#[cfg(test)]
#[path = "tls_config_test.rs"]
mod tests;
