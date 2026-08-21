use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use crate::terlan_hir::{syntax_module_output_to_interface, ModuleInterface};
use crate::terlan_syntax::{
    syntax_module_import_identities, SyntaxDeclarationPayload, SyntaxImportKind, SyntaxModuleOutput,
};

struct CachedDiscoveryCatalog {
    signature: u64,
    files: Vec<std::path::PathBuf>,
    symbols: HashMap<String, HashMap<String, ModuleInterface>>,
}

static DISCOVERY_CATALOGS: OnceLock<Mutex<HashMap<std::path::PathBuf, CachedDiscoveryCatalog>>> =
    OnceLock::new();

/// Parses one interface file into a module interface.
///
/// Inputs: path to `.terli` or `.typi`. Output: module name plus interface when
/// parsing succeeds. Transformation: reads source, parses interface syntax
/// output, and converts it to an interface summary.
pub fn parse_interface_file(path: &Path) -> Option<(String, ModuleInterface)> {
    let content = fs::read_to_string(path).ok()?;
    parse_interface_text(&content)
}

fn parse_interface_text(content: &str) -> Option<(String, ModuleInterface)> {
    let parsed = crate::terlan_syntax::parse_interface_module_as_syntax_output(content).ok()?;
    let module_name = parsed.module_name.clone();
    let interface = syntax_module_output_to_interface(&parsed);
    Some((module_name, interface))
}

/// Parses dependency entries from an interface dependency manifest.
///
/// Inputs:
/// - `contents`: line-oriented `.typi.deps` manifest text.
///
/// Output:
/// - Ordered module/hash entries, or `None` when the count or an entry is
///   malformed.
///
/// Transformation:
/// - Locates the declared dependency count and decodes exactly that many
///   structured `module=hash` entries.
pub fn parse_interface_dependency_entries(contents: &str) -> Option<Vec<(String, u64)>> {
    let mut lines = contents.lines();
    let count = lines
        .by_ref()
        .find_map(|line| line.strip_prefix("deps="))?
        .parse::<usize>()
        .ok()?;
    let entries = lines
        .by_ref()
        .take(count)
        .map(|line| {
            let (module_name, hash) = line.split_once('=')?;
            Some((module_name.to_string(), hash.parse::<u64>().ok()?))
        })
        .collect::<Option<Vec<_>>>()?;
    (entries.len() == count).then_some(entries)
}

/// Loads interface summaries from one directory.
///
/// Inputs: directory path and accumulator. Output: accumulator is updated.
/// Transformation: reads direct `.terli` and `.typi` files and inserts richer
/// duplicate summaries preferentially.
pub fn load_interfaces_from_dir(dir: &Path, acc: &mut HashMap<String, ModuleInterface>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        if extension == "terli" || extension == "typi" {
            if let Some((module_name, interface)) = parse_interface_file(&path) {
                insert_interface_if_not_poorer(acc, module_name, interface);
            }
        }
    }
}

/// Loads interfaces visible to one source file.
///
/// Inputs: source file path. Output: interface map. Transformation: scans the
/// source directory and nearest/std fallback trees for `.terli`/`.typi`
/// summaries.
pub fn load_interfaces_from_file_set(file_path: &str) -> HashMap<String, ModuleInterface> {
    let mut interfaces = HashMap::new();
    let current = Path::new(file_path);
    let base = current.parent().unwrap_or(Path::new("."));
    load_interfaces_from_dir(base, &mut interfaces);
    load_std_interfaces(current, &mut interfaces);
    interfaces
}

/// Loads interfaces that may expose one symbol for discovery operations.
///
/// Project-local summaries are reparsed on every call because an editor may
/// have just rebuilt them. Packaged summaries are text-filtered by identifier,
/// parsed only when they can contain the requested symbol, and cached by a
/// metadata-sealed catalog root.
pub fn load_discovery_interfaces_for_symbol_from_file_set(
    file_path: &str,
    symbol: &str,
) -> HashMap<String, ModuleInterface> {
    let mut interfaces = HashMap::new();
    let current = Path::new(file_path);
    let base = current.parent().unwrap_or(Path::new("."));
    load_interfaces_from_dir(base, &mut interfaces);

    let Some((std_dir, signature, files)) = find_std_catalog(current) else {
        return interfaces;
    };
    let cache_key = fs::canonicalize(&std_dir).unwrap_or(std_dir.clone());
    let catalogs_lock = DISCOVERY_CATALOGS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut catalogs = catalogs_lock.lock().expect("std interface catalog lock");
    let catalog = catalogs
        .entry(cache_key.clone())
        .and_modify(|catalog| {
            if catalog.signature != signature {
                *catalog = CachedDiscoveryCatalog {
                    signature,
                    files: files.clone(),
                    symbols: HashMap::new(),
                };
            }
        })
        .or_insert_with(|| CachedDiscoveryCatalog {
            signature,
            files,
            symbols: HashMap::new(),
        });
    let lookup = catalog
        .symbols
        .get(symbol)
        .cloned()
        .map_or_else(|| Err(catalog.files.clone()), Ok);
    drop(catalogs);

    let discovered = match lookup {
        Ok(cached) => cached,
        Err(files) => {
            let mut loaded = HashMap::new();
            for path in &files {
                let Ok(content) = fs::read_to_string(path) else {
                    continue;
                };
                if !text_contains_identifier(&content, symbol) {
                    continue;
                }
                if let Some((module_name, interface)) = parse_interface_text(&content) {
                    insert_interface_if_not_poorer(&mut loaded, module_name, interface);
                }
            }
            let mut catalogs = catalogs_lock.lock().expect("std interface catalog lock");
            let cached = catalogs
                .get_mut(&cache_key)
                .filter(|catalog| catalog.signature == signature)
                .map(|catalog| {
                    catalog
                        .symbols
                        .entry(symbol.to_string())
                        .or_insert_with(|| loaded.clone())
                        .clone()
                });
            cached.unwrap_or(loaded)
        }
    };

    for (module_name, interface) in discovered {
        insert_interface_if_not_poorer(&mut interfaces, module_name, interface);
    }
    interfaces
}

/// Reports whether text contains one complete ASCII-style identifier token.
fn text_contains_identifier(text: &str, identifier: &str) -> bool {
    if identifier.is_empty() {
        return false;
    }
    text.match_indices(identifier).any(|(start, _)| {
        let before = text[..start].bytes().next_back();
        let end = start + identifier.len();
        let after = text[end..].bytes().next();
        !before.is_some_and(is_identifier_byte) && !after.is_some_and(is_identifier_byte)
    })
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Loads only interfaces reachable from one parsed source module.
///
/// Inputs: source file path and parsed module. Output: local interfaces plus
/// directly imported packaged summaries and their declared dependency closure.
/// Transformation: avoids parsing the entire packaged interface catalog for
/// editor operations that can name their required providers.
pub fn load_imported_interfaces_from_file_set(
    file_path: &str,
    module: &SyntaxModuleOutput,
) -> HashMap<String, ModuleInterface> {
    let mut interfaces = HashMap::new();
    let current = Path::new(file_path);
    let base = current.parent().unwrap_or(Path::new("."));
    load_interfaces_from_dir(base, &mut interfaces);

    let Some(summaries) = find_std_summaries_dir(current) else {
        return interfaces;
    };

    let mut pending = syntax_module_import_identities(module)
        .into_iter()
        .collect::<Vec<_>>();
    pending.extend(collapsed_summary_import_candidates(module));
    let mut visited = HashSet::new();

    while let Some(module_name) = pending.pop() {
        if !visited.insert(module_name.clone()) {
            continue;
        }
        let path = summaries.join(format!("{module_name}.typi"));
        if let Some((parsed_name, interface)) = parse_interface_file(&path) {
            insert_interface_if_not_poorer(&mut interfaces, parsed_name, interface);
        }
        let manifest = summaries.join(format!("{module_name}.typi.deps"));
        pending.extend(
            fs::read_to_string(manifest)
                .ok()
                .and_then(|contents| parse_interface_dependency_entries(&contents))
                .unwrap_or_default()
                .into_iter()
                .map(|(dependency, _)| dependency),
        );
    }

    interfaces
}

/// Expands selected namespace imports whose items are packaged as modules.
fn collapsed_summary_import_candidates(module: &SyntaxModuleOutput) -> Vec<String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Module,
                module_name,
                items,
                ..
            } if items.len() > 1 => Some((module_name, items)),
            _ => None,
        })
        .flat_map(|(module_name, items)| {
            items
                .iter()
                .map(move |item| format!("{module_name}.{}", item.name))
        })
        .collect()
}

/// Inserts an interface without replacing a richer duplicate.
///
/// Inputs:
/// - `acc`: accumulated interfaces keyed by module name.
/// - `module_name`: module identity parsed from the interface file.
/// - `interface`: parsed interface candidate.
///
/// Output:
/// - `acc` contains the candidate when no existing interface is present or when
///   the candidate carries at least as much public surface as the existing one.
///
/// Transformation:
/// - Scores interfaces by public type, function, constructor, trait, and type
///   body payload counts, then ignores duplicate candidates that would erase a
///   richer summary discovered earlier in the same load pass.
fn insert_interface_if_not_poorer(
    acc: &mut HashMap<String, ModuleInterface>,
    module_name: String,
    interface: ModuleInterface,
) {
    let incoming_score = interface_payload_score(&interface);
    let existing_score = acc
        .get(&module_name)
        .map(interface_payload_score)
        .unwrap_or(0);
    if incoming_score >= existing_score {
        acc.insert(module_name, interface);
    }
}

/// Computes a coarse public-payload score for duplicate interface resolution.
///
/// Inputs:
/// - `interface`: parsed interface candidate.
///
/// Output:
/// - Count of public surface payload buckets present in the interface.
///
/// Transformation:
/// - Sums exported type, opaque/private type, type body, trait, constructor,
///   and function counts so duplicate resolution prefers the interface with
///   more usable compiler metadata.
fn interface_payload_score(interface: &ModuleInterface) -> usize {
    interface.public_types.len()
        + interface.private_types.len()
        + interface.opaque_types.len()
        + interface.type_bodies.len()
        + interface.traits.len()
        + interface.constructors.len()
        + interface.functions.len()
}

/// Loads standard-library interfaces visible from a source path.
///
/// Inputs: current source path and accumulator. Output: accumulator is updated.
/// Transformation: walks upward looking for a `std` tree, falling back to
/// `./std` from the current working directory.
fn load_std_interfaces(current: &Path, acc: &mut HashMap<String, ModuleInterface>) {
    let mut dir = current.parent();
    while let Some(candidate) = dir {
        let std_dir = candidate.join("std");
        if std_dir.is_dir() && load_interfaces_from_std_tree(&std_dir, acc) > 0 {
            return;
        }
        dir = candidate.parent();
    }

    let cwd_std = Path::new("std");
    if cwd_std.is_dir() {
        load_interfaces_from_std_tree(cwd_std, acc);
    }
}

/// Finds the nearest packaged summary directory, with the cwd fallback.
fn find_std_summaries_dir(current: &Path) -> Option<std::path::PathBuf> {
    let mut dir = current.parent();
    while let Some(candidate) = dir {
        let summaries = candidate.join("std/summaries");
        if summaries.is_dir() {
            return Some(summaries);
        }
        dir = candidate.parent();
    }

    let cwd_summaries = Path::new("std/summaries");
    cwd_summaries.is_dir().then(|| cwd_summaries.to_path_buf())
}

/// Finds an ancestor std tree and computes its deterministic freshness seal.
///
/// Discovery deliberately has no cwd fallback: an unrelated editor workspace
/// must not inherit auto-import candidates from the LSP process launch path.
fn find_std_catalog(current: &Path) -> Option<(std::path::PathBuf, u64, Vec<std::path::PathBuf>)> {
    let mut dir = current.parent();
    while let Some(candidate) = dir {
        let std_dir = candidate.join("std");
        if let Some((signature, files)) = std_tree_inventory(&std_dir) {
            return Some((std_dir, signature, files));
        }
        dir = candidate.parent();
    }
    None
}

/// Seals the file names, sizes, and mtimes consumed by std-tree discovery.
fn std_tree_inventory(std_dir: &Path) -> Option<(u64, Vec<std::path::PathBuf>)> {
    let mut entries = fs::read_dir(std_dir)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .flat_map(|entry| fs::read_dir(entry.path()).into_iter().flatten().flatten())
        .filter_map(|entry| {
            let path = entry.path();
            let extension = path.extension().and_then(|value| value.to_str());
            if !matches!(extension, Some("terli" | "typi")) {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            let modified = metadata
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_nanos();
            Some((path, metadata.len(), modified))
        })
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return None;
    }
    entries.sort_unstable();

    let mut signature = 0xcbf2_9ce4_8422_2325_u64;
    let mut files = Vec::with_capacity(entries.len());
    for (path, size, modified) in entries {
        files.push(path.clone());
        for byte in path
            .to_string_lossy()
            .bytes()
            .chain(size.to_le_bytes())
            .chain(modified.to_le_bytes())
        {
            signature ^= u64::from(byte);
            signature = signature.wrapping_mul(0x100_0000_01b3);
        }
    }
    Some((signature, files))
}

/// Loads interfaces from a standard-library tree.
///
/// Inputs: std root and accumulator. Output: number of newly added interfaces.
/// Transformation: scans child directories for interface files using the same
/// directory loader as project sources.
fn load_interfaces_from_std_tree(
    std_dir: &Path,
    acc: &mut HashMap<String, ModuleInterface>,
) -> usize {
    let before = acc.len();
    let entries = match fs::read_dir(std_dir) {
        Ok(entries) => entries,
        Err(_) => return 0,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_interfaces_from_dir(&path, acc);
        }
    }

    acc.len().saturating_sub(before)
}
