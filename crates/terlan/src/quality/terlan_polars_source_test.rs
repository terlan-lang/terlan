use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Proves an explicit local package override has highest precedence.
#[test]
fn source_resolution_prefers_explicit_package_directory() {
    let fixture = SourceFixture::new("explicit");
    let explicit = fixture.root.join("explicit-package");
    fs::create_dir_all(&explicit).expect("create explicit package");
    let config = fixture.config(Some(explicit.clone()), None, None);

    assert_eq!(resolve_source(&config).expect("resolve explicit"), explicit);
}

/// Proves a sibling checkout is used without requiring Git publication.
#[test]
fn source_resolution_uses_sibling_without_revision() {
    let fixture = SourceFixture::new("sibling");
    fs::create_dir_all(&fixture.sibling).expect("create sibling package");
    let config = fixture.config(None, None, None);

    assert_eq!(
        resolve_source(&config).expect("resolve sibling"),
        fixture.sibling
    );
}

/// Proves nonlocal resolution rejects floating or abbreviated revisions.
#[test]
fn source_resolution_requires_full_revision() {
    let fixture = SourceFixture::new("invalid-revision");
    let config = fixture.config(None, Some(fixture.root.as_os_str()), Some("abc123"));

    let error = resolve_source(&config).expect_err("reject short revision");

    assert!(error.contains("error[terlan_polars_revision_invalid]"));
}

/// Proves a configured immutable source materializes once and then replays offline.
#[test]
fn source_resolution_materializes_and_reuses_revision_cache() {
    let fixture = SourceFixture::new("materialize");
    let source = fixture.root.join("source");
    let revision = create_git_source(&source);
    let config = fixture.config(None, Some(source.as_os_str()), Some(&revision));

    let first = resolve_source(&config).expect("materialize configured source");
    fs::remove_dir_all(&source).expect("remove source to prove cache replay");
    let offline = fixture.config(None, None, Some(&revision));
    let second = resolve_source(&offline).expect("reuse cache without source");

    assert_eq!(first, second);
    assert_eq!(
        git_stdout(&second, ["rev-parse", "HEAD"], "test").expect("cached head"),
        revision
    );
}

/// Proves a cache entry at the requested address cannot hide another revision.
#[test]
fn source_resolution_rejects_revision_mismatched_cache() {
    let fixture = SourceFixture::new("poisoned-cache");
    let source = fixture.root.join("source");
    let actual_revision = create_git_source(&source);
    let requested_revision = "1111111111111111111111111111111111111111";
    let poisoned = fixture.cache.join(requested_revision);
    fs::create_dir_all(&fixture.cache).expect("create cache root");
    run_git(
        fixture.root.as_path(),
        [
            "clone",
            "--quiet",
            source.to_str().expect("source utf8"),
            poisoned.to_str().expect("cache utf8"),
        ],
    );
    assert_ne!(actual_revision, requested_revision);
    let config = fixture.config(None, None, Some(requested_revision));

    let error = resolve_source(&config).expect_err("reject poisoned cache");

    assert!(error.contains("error[terlan_polars_cache_revision_mismatch]"));
}

struct SourceFixture {
    root: PathBuf,
    sibling: PathBuf,
    cache: PathBuf,
}

impl SourceFixture {
    fn new(name: &str) -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = env::temp_dir().join(format!(
            "terlan-polars-source-{name}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create source fixture");
        Self {
            sibling: root.join("missing-sibling"),
            cache: root.join("cache"),
            root,
        }
    }

    fn config(
        &self,
        explicit_dir: Option<PathBuf>,
        source: Option<&OsStr>,
        revision: Option<&str>,
    ) -> TerlanPolarsSourceConfig {
        TerlanPolarsSourceConfig {
            explicit_dir,
            sibling_dir: self.sibling.clone(),
            source: source.map(OsStr::to_os_string),
            revision: revision.map(str::to_string),
            cache_root: self.cache.clone(),
        }
    }
}

impl Drop for SourceFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove source fixture");
    }
}

fn create_git_source(path: &Path) -> String {
    fs::create_dir_all(path).expect("create git source");
    run_git(path, ["init", "--quiet"]);
    run_git(path, ["config", "user.name", "Terlan Quality"]);
    run_git(path, ["config", "user.email", "quality@terlan.invalid"]);
    fs::write(
        path.join("terlan.toml"),
        "[package]\nname = \"terlan-polars\"\n",
    )
    .expect("write source file");
    run_git(path, ["add", "terlan.toml"]);
    run_git(path, ["commit", "--quiet", "-m", "fixture"]);
    git_stdout(path, ["rev-parse", "HEAD"], "test").expect("source revision")
}

fn run_git<const N: usize>(directory: &Path, args: [&str; N]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .expect("execute git fixture command");
    assert!(
        output.status.success(),
        "git fixture command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
