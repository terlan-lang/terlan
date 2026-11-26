use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// One source module available to a compiler planning operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceModule {
    /// Canonical source-level module identity.
    pub identity: String,
    /// Source root used to validate the module layout.
    pub root: PathBuf,
    /// Concrete implementation or interface source path.
    pub path: PathBuf,
}

/// Two distinct source paths declared the same module identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateSourceModule {
    /// Conflicting canonical module identity.
    pub identity: String,
    /// First path indexed for the identity.
    pub first: PathBuf,
    /// Later conflicting path.
    pub second: PathBuf,
}

impl Display for DuplicateSourceModule {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "duplicate project source module `{}` at {} and {}",
            self.identity,
            self.first.display(),
            self.second.display()
        )
    }
}

impl Error for DuplicateSourceModule {}

/// Deterministic lookup and transitive traversal for source modules.
#[derive(Debug, Clone, Default)]
pub struct SourceModuleIndex {
    modules: BTreeMap<String, SourceModule>,
}

impl SourceModuleIndex {
    /// Indexes module sources, preferring an explicit `.terli` sibling.
    ///
    /// Distinct paths with the same module identity are rejected. A `.terl`
    /// implementation and `.terli` contract with the same stem are one source
    /// module, regardless of their discovery order.
    pub fn new(modules: Vec<SourceModule>) -> Result<Self, DuplicateSourceModule> {
        let mut indexed = BTreeMap::<String, SourceModule>::new();
        for module in modules {
            let Some(previous) = indexed.get(&module.identity) else {
                indexed.insert(module.identity.clone(), module);
                continue;
            };
            if previous.path.with_extension("") != module.path.with_extension("") {
                return Err(DuplicateSourceModule {
                    identity: module.identity,
                    first: previous.path.clone(),
                    second: module.path,
                });
            }
            if module.path.extension() == Some(OsStr::new("terli")) {
                indexed.insert(module.identity.clone(), module);
            }
        }
        Ok(Self { modules: indexed })
    }

    /// Returns the number of unique indexed module identities.
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Reports whether the index contains no module identities.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Resolves the transitive closure reachable from initial module imports.
    ///
    /// Missing imports are external to this index and are deliberately
    /// ignored. `imports` is invoked once for each reachable indexed module,
    /// so callers never need to parse or load unreachable source files.
    pub fn transitive_closure(
        &self,
        initial: Vec<String>,
        imports: &mut dyn FnMut(&SourceModule) -> Result<Vec<String>, String>,
    ) -> Result<Vec<SourceModule>, String> {
        let mut pending = VecDeque::from_iter(initial);
        let mut seen = BTreeSet::new();
        let mut selected = Vec::new();
        while let Some(identity) = pending.pop_front() {
            if !seen.insert(identity.clone()) {
                continue;
            }
            let Some(module) = self.modules.get(&identity) else {
                continue;
            };
            pending.extend(imports(module)?);
            selected.push(module.clone());
        }
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(identity: &str, root: &str, path: &str) -> SourceModule {
        SourceModule {
            identity: identity.into(),
            root: root.into(),
            path: path.into(),
        }
    }

    #[test]
    fn indexes_unique_modules_and_reports_empty_state() {
        let empty = SourceModuleIndex::default();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);

        let index = SourceModuleIndex::new(vec![
            module("app.First", "src", "src/app/First.terl"),
            module("app.Second", "src", "src/app/Second.terl"),
        ])
        .expect("unique modules");
        assert!(!index.is_empty());
        assert_eq!(index.len(), 2);
    }

    #[test]
    fn explicit_interface_wins_in_both_discovery_orders() {
        for entries in [
            vec![
                module("app.Value", "src", "src/app/Value.terl"),
                module("app.Value", "src", "src/app/Value.terli"),
            ],
            vec![
                module("app.Value", "src", "src/app/Value.terli"),
                module("app.Value", "src", "src/app/Value.terl"),
            ],
        ] {
            let index = SourceModuleIndex::new(entries).expect("source siblings");
            let selected = index
                .transitive_closure(vec!["app.Value".into()], &mut |_| Ok(Vec::new()))
                .expect("source closure");
            assert_eq!(selected[0].path, PathBuf::from("src/app/Value.terli"));
        }
    }

    #[test]
    fn rejects_distinct_paths_with_one_identity() {
        let error = SourceModuleIndex::new(vec![
            module("app.Value", "left", "left/app/Value.terl"),
            module("app.Value", "right", "right/app/Value.terl"),
        ])
        .expect_err("duplicate identity");
        assert_eq!(error.identity, "app.Value");
        assert_eq!(error.first, PathBuf::from("left/app/Value.terl"));
        assert_eq!(error.second, PathBuf::from("right/app/Value.terl"));
        assert_eq!(
            error.to_string(),
            "duplicate project source module `app.Value` at left/app/Value.terl and right/app/Value.terl"
        );
    }

    #[test]
    fn traverses_only_reachable_modules_once() {
        let index = SourceModuleIndex::new(vec![
            module("app.Entry", "src", "src/app/Entry.terl"),
            module("app.Shared", "src", "src/app/Shared.terl"),
            module("app.Leaf", "src", "src/app/Leaf.terl"),
            module("app.Unused", "src", "src/app/Unused.terl"),
        ])
        .expect("source index");
        let mut loaded = Vec::new();
        let selected = index
            .transitive_closure(
                vec!["external.Runtime".into(), "app.Entry".into()],
                &mut |module| {
                    loaded.push(module.identity.clone());
                    Ok(match module.identity.as_str() {
                        "app.Entry" => vec!["app.Shared".into(), "app.Leaf".into()],
                        "app.Shared" => vec!["app.Leaf".into(), "app.Entry".into()],
                        _ => Vec::new(),
                    })
                },
            )
            .expect("transitive source closure");
        assert_eq!(loaded, ["app.Entry", "app.Shared", "app.Leaf"]);
        assert_eq!(
            selected
                .into_iter()
                .map(|module| module.identity)
                .collect::<Vec<_>>(),
            ["app.Entry", "app.Shared", "app.Leaf"]
        );
    }

    #[test]
    fn propagates_reachable_import_loading_errors() {
        let index =
            SourceModuleIndex::new(vec![module("app.Broken", "src", "src/app/Broken.terl")])
                .expect("source index");
        assert_eq!(
            index.transitive_closure(vec!["app.Broken".into()], &mut |_| Err(
                "parse failure".into()
            )),
            Err("parse failure".into())
        );
    }
}
