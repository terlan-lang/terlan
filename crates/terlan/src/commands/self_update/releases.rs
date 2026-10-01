use super::UpdateError;

use semver::Version;
use serde::Deserialize;

const API: &str = "https://api.github.com/repos/terlan-lang/terlan/releases";

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Release {
    pub(super) tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize)]
struct Asset {
    name: String,
}

impl Release {
    /// Classifies prereleases using both GitHub metadata and semantic versioning.
    pub(super) fn is_prerelease(&self) -> bool {
        self.prerelease || version(&self.tag_name).is_ok_and(|version| !version.pre.is_empty())
    }

    /// Admits only published version tags with both the bundle and its checksum.
    fn compatible(&self, artifact: &str) -> bool {
        !self.draft
            && version(&self.tag_name).is_ok()
            && self.assets.iter().any(|asset| asset.name == artifact)
            && self
                .assets
                .iter()
                .any(|asset| asset.name == format!("{artifact}.sha256"))
    }
}

/// Parses tags accepted by the release installers, excluding path/command syntax.
pub(super) fn version(tag: &str) -> Result<Version, UpdateError> {
    let value = tag
        .strip_prefix('v')
        .ok_or_else(|| format!("invalid release tag: {tag}"))?;
    // Installers do not accept SemVer build metadata (+suffix).
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
    {
        return Err(format!("invalid release version: {tag}").into());
    }
    Version::parse(value).map_err(|_| format!("invalid release version: {tag}").into())
}

/// Maps the running compiler's platform to the published archive naming scheme.
pub(super) fn platform_artifact(os: &str, arch: &str) -> Result<String, UpdateError> {
    let (platform, extension) = match os {
        "linux" => ("linux", "tar.gz"),
        "macos" => ("macos", "tar.gz"),
        "windows" => ("windows", "zip"),
        _ => return Err(format!("unsupported update platform: {os}").into()),
    };
    if !matches!(arch, "x86_64" | "aarch64") {
        return Err(format!("unsupported update architecture: {arch}").into());
    }
    Ok(format!("terlc-{platform}-{arch}.{extension}"))
}

pub(super) struct Client {
    base: String,
}

impl Client {
    /// Creates a client scoped to the project's official GitHub releases.
    pub(super) fn github() -> Self {
        Self { base: API.into() }
    }

    /// Uses GitHub's designated latest stable release, rather than list ordering.
    pub(super) fn latest(&self, artifact: &str) -> Result<Release, UpdateError> {
        let release: Release = self.get("/latest")?;
        if release.is_prerelease() || !release.compatible(artifact) {
            return Err(format!("latest stable release has no verified bundle for {artifact}; use --list to select another version").into());
        }
        Ok(release)
    }

    /// Resolves an explicit version, including an intentionally selected prerelease.
    pub(super) fn by_tag(&self, tag: &str, artifact: &str) -> Result<Release, UpdateError> {
        version(tag)?;
        let release: Release = self.get(&format!("/tags/{tag}"))?;
        if release.tag_name != tag || !release.compatible(artifact) {
            return Err(format!("release {tag} has no verified bundle for {artifact}; use --list to see available versions").into());
        }
        Ok(release)
    }

    /// Paginates published releases, filters by platform, and sorts newest versions first.
    pub(super) fn list(&self, artifact: &str) -> Result<Vec<Release>, UpdateError> {
        let mut releases = Vec::new();
        for page in 1..=100 {
            let batch: Vec<Release> = self.get(&format!("?per_page=100&page={page}"))?;
            let finished = batch.len() < 100;
            releases.extend(
                batch
                    .into_iter()
                    .filter(|release| release.compatible(artifact)),
            );
            if finished {
                releases.sort_by_cached_key(|release| {
                    std::cmp::Reverse(version(&release.tag_name).ok())
                });
                releases.dedup_by(|left, right| left.tag_name == right.tag_name);
                return if releases.is_empty() {
                    Err(format!("no published releases include {artifact} and its checksum").into())
                } else {
                    Ok(releases)
                };
            }
        }
        Err("GitHub release listing exceeded 100 pages; specify a version explicitly".into())
    }

    /// Fetches bounded JSON with timeouts and actionable GitHub HTTP errors.
    #[cfg(feature = "registry-network")]
    fn get<T: serde::de::DeserializeOwned>(&self, route: &str) -> Result<T, UpdateError> {
        use std::io::Read;
        use std::time::Duration;
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(30)))
                .max_redirects(0)
                .http_status_as_error(false)
                .build(),
        );
        let mut response = agent
            .get(format!("{}{route}", self.base))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", concat!("terlc/", env!("CARGO_PKG_VERSION")))
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(|error| format!("could not reach GitHub: {error}"))?;
        match response.status().as_u16() {
            200 => {}
            404 => {
                return Err("GitHub release not found; use --list to see published versions".into())
            }
            403 | 429 => {
                return Err(
                    "GitHub denied the request or its API rate limit was reached; try again later"
                        .into(),
                )
            }
            status => return Err(format!("GitHub returned HTTP {status}").into()),
        }
        const LIMIT: u64 = 8 * 1024 * 1024;
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("could not read GitHub response: {error}"))?;
        if bytes.len() as u64 > LIMIT {
            return Err("GitHub response exceeded 8 MiB".into());
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid GitHub release response: {error}").into())
    }

    /// Keeps minimal offline compiler builds usable with an explicit update diagnostic.
    #[cfg(not(feature = "registry-network"))]
    fn get<T: serde::de::DeserializeOwned>(&self, _route: &str) -> Result<T, UpdateError> {
        Err(format!(
            "this build cannot access {}; rebuild with the registry-network feature",
            self.base
        )
        .into())
    }

    #[cfg(all(test, feature = "registry-network"))]
    pub(super) fn for_test(base: String) -> Self {
        Self { base }
    }
}
