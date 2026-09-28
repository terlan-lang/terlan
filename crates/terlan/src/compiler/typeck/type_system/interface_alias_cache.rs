//! Reuses parsed interface aliases within a compiler thread, with bounded retention.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};

use super::{ModuleInterface, TypeAlias};

const MAX_ENTRIES: usize = 32;
const MAX_SOURCE_WEIGHT: usize = 1024 * 1024;

thread_local! {
    static CACHE: RefCell<VecDeque<CachedInterfaceAliases>> = const { RefCell::new(VecDeque::new()) };
}

struct CachedInterfaceAliases {
    module: String,
    public_types: HashSet<String>,
    opaque_types: HashSet<String>,
    type_params: HashMap<String, Vec<String>>,
    type_bodies: HashMap<String, Vec<String>>,
    valued_unions: HashSet<String>,
    aliases: HashMap<String, TypeAlias>,
    source_weight: usize,
}

impl CachedInterfaceAliases {
    fn matches(&self, interface: &ModuleInterface) -> bool {
        self.module == interface.module
            && self.public_types == interface.public_types
            && self.opaque_types == interface.opaque_types
            && self.type_params == interface.type_params
            && self.type_bodies == interface.type_bodies
            && self.valued_unions.len() == interface.valued_unions.len()
            && interface
                .valued_unions
                .keys()
                .all(|name| self.valued_unions.contains(name))
    }
}

// Bound both source text and collection cardinality, including empty strings.
// Parsed aliases are derived from this bounded text; this is a retention weight,
// not a claim about the allocator's exact resident byte count.
fn source_weight(interface: &ModuleInterface) -> usize {
    std::iter::once(&interface.module)
        .chain(interface.public_types.iter())
        .chain(interface.opaque_types.iter())
        .chain(interface.valued_unions.keys())
        .chain(
            interface
                .type_params
                .iter()
                .chain(interface.type_bodies.iter())
                .flat_map(|(name, values)| std::iter::once(name).chain(values.iter())),
        )
        .fold(0usize, |weight, text| {
            weight.saturating_add(text.len().saturating_add(64))
        })
}

/// Returns owned aliases, reusing only an interface with identical parser inputs.
pub(super) fn get_or_parse(
    interface: &ModuleInterface,
    parse: impl FnOnce() -> HashMap<String, TypeAlias>,
) -> HashMap<String, TypeAlias> {
    let cached = CACHE.with_borrow_mut(|entries| {
        let index = entries.iter().position(|entry| entry.matches(interface))?;
        let entry = entries.remove(index)?;
        let aliases = entry.aliases.clone();
        entries.push_back(entry);
        Some(aliases)
    });
    if let Some(aliases) = cached {
        return aliases;
    }

    // Parsing does not hold a cache borrow. Callers own their returned map and
    // may qualify or expand its aliases without changing the retained values.
    let aliases = parse();
    let weight = source_weight(interface);
    if weight <= MAX_SOURCE_WEIGHT {
        CACHE.with_borrow_mut(|entries| {
            while entries.len() >= MAX_ENTRIES
                || entries
                    .iter()
                    .map(|entry| entry.source_weight)
                    .sum::<usize>()
                    + weight
                    > MAX_SOURCE_WEIGHT
            {
                entries.pop_front();
            }
            entries.push_back(CachedInterfaceAliases {
                module: interface.module.clone(),
                public_types: interface.public_types.clone(),
                opaque_types: interface.opaque_types.clone(),
                type_params: interface.type_params.clone(),
                type_bodies: interface.type_bodies.clone(),
                valued_unions: interface.valued_unions.keys().cloned().collect(),
                aliases: aliases.clone(),
                source_weight: weight,
            });
        });
    }
    aliases
}

#[cfg(test)]
#[path = "interface_alias_cache_test.rs"]
mod tests;
