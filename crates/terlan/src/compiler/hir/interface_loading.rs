use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::terlan_hir::{syntax_module_output_to_interface, ModuleInterface};

/// Parses one interface file into a module interface.
///
/// Inputs: path to `.terli` or `.typi`. Output: module name plus interface when
/// parsing succeeds. Transformation: reads source, parses interface syntax
/// output, and converts it to an interface summary.
pub fn parse_interface_file(path: &Path) -> Option<(String, ModuleInterface)> {
    let content = fs::read_to_string(path).ok()?;
    let parsed = crate::terlan_syntax::parse_interface_module_as_syntax_output(&content).ok()?;
    let module_name = parsed.module_name.clone();
    let interface = syntax_module_output_to_interface(&parsed);
    Some((module_name, interface))
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
