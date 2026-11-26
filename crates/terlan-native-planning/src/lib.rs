//! Bounded, independently testable compiler planning primitives.

use std::collections::{HashMap, HashSet};

mod progress;
mod source_closure;

pub use progress::{ProgressEvent, ProgressWatch};
pub use source_closure::{DuplicateSourceModule, SourceModule, SourceModuleIndex};

/// Canonical module, function name, and arity for one callable provider.
pub type FunctionKey = (String, String, usize);

/// Precomputed provider lookups used during native reachability planning.
#[derive(Default)]
pub struct ProviderIndex {
    providers: Vec<FunctionKey>,
    exact: HashSet<FunctionKey>,
    remote: HashMap<String, HashMap<String, HashMap<usize, usize>>>,
    qualified: HashMap<String, HashMap<usize, usize>>,
    canonical: HashMap<String, HashMap<usize, usize>>,
    named: HashMap<String, HashMap<usize, Vec<usize>>>,
    values: HashMap<String, Vec<usize>>,
}

impl ProviderIndex {
    /// Adds providers without duplicating an existing callable identity.
    pub fn extend(&mut self, providers: Vec<FunctionKey>) {
        for provider in providers {
            if !self.exact.insert(provider.clone()) {
                continue;
            }
            let index = self.providers.len();
            let (module, function, arity) = &provider;
            let qualified = format!("{module}.{function}");
            self.qualified
                .entry(qualified.clone())
                .or_default()
                .entry(*arity)
                .or_insert(index);
            self.canonical
                .entry(canonical_callable_path(&qualified))
                .or_default()
                .entry(*arity)
                .or_insert(index);
            self.remote
                .entry(module.clone())
                .or_default()
                .entry(function.clone())
                .or_default()
                .entry(*arity)
                .or_insert(index);
            self.named
                .entry(function.clone())
                .or_default()
                .entry(*arity)
                .or_default()
                .push(index);
            self.values.entry(function.clone()).or_default().push(index);
            self.providers.push(provider);
        }
    }

    /// Reports whether the exact callable identity is indexed.
    pub fn contains(&self, provider: &FunctionKey) -> bool {
        self.exact.contains(provider)
    }

    /// Resolves a local, imported, qualified, or canonical callable in scope.
    pub fn resolve_scoped(
        &self,
        caller_module: &str,
        imported: &HashSet<&str>,
        name: &str,
        arity: usize,
    ) -> Option<FunctionKey> {
        if let Some(local) = self.resolve_remote(caller_module, name, arity) {
            return Some(local);
        }
        if name.contains('.') {
            return self
                .qualified
                .get(name)
                .and_then(|arities| arities.get(&arity))
                .or_else(|| {
                    self.canonical
                        .get(&canonical_callable_path(name))
                        .and_then(|arities| arities.get(&arity))
                })
                .map(|index| self.providers[*index].clone());
        }
        let mut matches = self.named.get(name)?.get(&arity)?.iter().filter(|index| {
            let (module, function, _) = &self.providers[**index];
            imported.contains(module.as_str())
                || imported.contains(format!("{module}.{function}").as_str())
        });
        let first = *matches.next()?;
        matches
            .next()
            .is_none()
            .then(|| self.providers[first].clone())
    }

    /// Resolves an exact module-qualified callable.
    pub fn resolve_remote(
        &self,
        module: &str,
        function: &str,
        arity: usize,
    ) -> Option<FunctionKey> {
        self.remote
            .get(module)?
            .get(function)?
            .get(&arity)
            .map(|index| self.providers[*index].clone())
    }

    /// Iterates every provider that exposes the requested value name.
    pub fn value_candidates<'a>(&'a self, name: &str) -> impl Iterator<Item = &'a FunctionKey> {
        self.values
            .get(name)
            .into_iter()
            .flatten()
            .map(|index| &self.providers[*index])
    }
}

fn canonical_callable_path(path: &str) -> String {
    let Some((owner, method)) = path.rsplit_once('.') else {
        return path.to_string();
    };
    let Some((parent, leaf)) = owner.rsplit_once('.') else {
        return path.to_string();
    };
    if parent.rsplit('.').next() == Some(leaf) {
        format!("{parent}.{method}")
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(module: &str, function: &str, arity: usize) -> FunctionKey {
        (module.into(), function.into(), arity)
    }

    fn index() -> ProviderIndex {
        let mut index = ProviderIndex::default();
        index.extend(vec![
            key("app.Main", "local", 0),
            key("std.alpha.Codec", "encode", 1),
            key("std.binary.Codec", "encode", 1),
            key("std.core.Show.Show", "show", 1),
            key("std.values", "callback", 0),
            key("std.values", "callback", 1),
        ]);
        index
    }

    #[test]
    fn duplicate_insertion_is_idempotent() {
        let mut index = index();
        let duplicate = key("app.Main", "local", 0);
        index.extend(vec![duplicate.clone()]);
        assert!(index.contains(&duplicate));
        assert!(!index.contains(&key("app.Main", "missing", 0)));
    }

    #[test]
    fn local_and_qualified_resolution_are_constant_scope_lookups() {
        let index = index();
        assert_eq!(
            index.resolve_scoped("app.Main", &HashSet::new(), "local", 0),
            Some(key("app.Main", "local", 0))
        );
        assert_eq!(
            index.resolve_scoped("app.Main", &HashSet::new(), "std.alpha.Codec.encode", 1,),
            Some(key("std.alpha.Codec", "encode", 1))
        );
        assert_eq!(
            index.resolve_scoped("app.Main", &HashSet::new(), "std.core.Show.show", 1),
            Some(key("std.core.Show.Show", "show", 1))
        );
        assert_eq!(
            index.resolve_scoped("app.Main", &HashSet::new(), "missing.path", 1),
            None
        );
    }

    #[test]
    fn unqualified_resolution_requires_one_visible_provider() {
        let index = index();
        assert_eq!(
            index.resolve_scoped("app.Main", &HashSet::from(["std.alpha.Codec"]), "encode", 1,),
            Some(key("std.alpha.Codec", "encode", 1))
        );
        assert_eq!(
            index.resolve_scoped(
                "app.Main",
                &HashSet::from(["std.alpha.Codec.encode"]),
                "encode",
                1,
            ),
            Some(key("std.alpha.Codec", "encode", 1))
        );
        assert_eq!(
            index.resolve_scoped(
                "app.Main",
                &HashSet::from(["std.alpha.Codec", "std.binary.Codec"]),
                "encode",
                1,
            ),
            None
        );
        assert_eq!(
            index.resolve_scoped("app.Main", &HashSet::new(), "unknown", 1),
            None
        );
        assert_eq!(
            index.resolve_scoped("app.Main", &HashSet::new(), "encode", 1),
            None
        );
        assert_eq!(
            index.resolve_scoped("app.Main", &HashSet::new(), "encode", 2),
            None
        );
    }

    #[test]
    fn remote_and_value_queries_reuse_the_index() {
        let index = index();
        assert_eq!(
            index.resolve_remote("std.alpha.Codec", "encode", 1),
            Some(key("std.alpha.Codec", "encode", 1))
        );
        assert_eq!(index.resolve_remote("std.alpha.Codec", "encode", 2), None);
        assert_eq!(index.resolve_remote("std.alpha.Codec", "missing", 1), None);
        assert_eq!(index.resolve_remote("missing", "encode", 1), None);
        assert_eq!(
            index
                .value_candidates("callback")
                .cloned()
                .collect::<Vec<_>>(),
            [
                key("std.values", "callback", 0),
                key("std.values", "callback", 1)
            ]
        );
        assert_eq!(index.value_candidates("missing").count(), 0);
    }

    #[test]
    fn short_callable_paths_remain_unchanged() {
        assert_eq!(canonical_callable_path("plain"), "plain");
        assert_eq!(canonical_callable_path("owner.call"), "owner.call");
        assert_eq!(canonical_callable_path("a.B.call"), "a.B.call");
    }
}
