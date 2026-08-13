use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget, SymbolSection};
use std::process::Command;

fn symbol(module: &str, function: &str, arity: usize) -> String {
    super::super::symbol::native_symbol(module, function, arity)
}

pub(super) fn assert_no_self_relocation(object: &[u8]) {
    assert_no_component_relocation(object, &[("loop", 2)]);
}

pub(super) fn assert_no_component_relocation(object: &[u8], functions: &[(&str, usize)]) {
    let parsed = object::File::parse(object).expect("parse direct tail-loop object");
    let component_symbols = functions
        .iter()
        .map(|(function, arity)| symbol("app.Tail", function, *arity))
        .collect::<Vec<_>>();
    for symbol_name in &component_symbols {
        let symbol = parsed
            .symbols()
            .find(|symbol| symbol.name().ok() == Some(symbol_name.as_str()))
            .expect("find tail-loop component symbol");
        let SymbolSection::Section(section_index) = symbol.section() else {
            panic!("tail-loop symbol must belong to a code section")
        };
        let section = parsed
            .section_by_index(section_index)
            .expect("read tail-loop code section");
        let start = symbol.address().saturating_sub(section.address());
        let end = start.saturating_add(symbol.size());
        for (offset, relocation) in section.relocations() {
            if offset < start || offset >= end {
                continue;
            }
            let RelocationTarget::Symbol(target) = relocation.target() else {
                continue;
            };
            let target = parsed
                .symbol_by_index(target)
                .expect("read relocation target");
            assert!(
                !component_symbols
                    .iter()
                    .any(|component| target.name().ok() == Some(component.as_str())),
                "recursive component must use a dispatcher backedge, not relocation to {:?}",
                target.name().ok()
            );
        }
    }
}

pub(super) fn assert_component_relocation(object: &[u8], function: &str, arity: usize) {
    let parsed = object::File::parse(object).expect("parse non-tail recursive object");
    let symbol_name = symbol("app.Tail", function, arity);
    let symbol = parsed
        .symbols()
        .find(|symbol| symbol.name().ok() == Some(symbol_name.as_str()))
        .expect("find non-tail recursive symbol");
    let SymbolSection::Section(section_index) = symbol.section() else {
        panic!("non-tail recursive symbol must belong to a code section")
    };
    let section = parsed
        .section_by_index(section_index)
        .expect("read non-tail recursive code section");
    let start = symbol.address().saturating_sub(section.address());
    let end = start.saturating_add(symbol.size());
    let has_self_call = section.relocations().any(|(offset, relocation)| {
        if offset < start || offset >= end {
            return false;
        }
        let RelocationTarget::Symbol(target) = relocation.target() else {
            return false;
        };
        parsed
            .symbol_by_index(target)
            .ok()
            .and_then(|target| target.name().ok())
            == Some(symbol_name.as_str())
    });
    assert!(
        has_self_call,
        "non-tail recursive result consumer must retain a real self-call relocation"
    );
}

pub(super) fn assert_defined_component_has_no_recursive_relocation(
    object: &[u8],
    module: &str,
    function: &str,
    arity: usize,
    component: &[(&str, &str, usize)],
) {
    let parsed = object::File::parse(object).expect("parse split component object");
    let symbol_name = symbol(module, function, arity);
    let component_symbols = component
        .iter()
        .map(|(module, function, arity)| symbol(module, function, *arity))
        .collect::<Vec<_>>();
    let symbol = parsed
        .symbols()
        .find(|symbol| symbol.name().ok() == Some(symbol_name.as_str()))
        .expect("find split component symbol");
    let SymbolSection::Section(section_index) = symbol.section() else {
        panic!("split component symbol must belong to a code section")
    };
    let section = parsed
        .section_by_index(section_index)
        .expect("read split component code section");
    let start = symbol.address().saturating_sub(section.address());
    let end = start.saturating_add(symbol.size());
    for (offset, relocation) in section.relocations() {
        if offset < start || offset >= end {
            continue;
        }
        let RelocationTarget::Symbol(target) = relocation.target() else {
            continue;
        };
        let target = parsed
            .symbol_by_index(target)
            .expect("read split component relocation target");
        assert!(
            !component_symbols
                .iter()
                .any(|component| target.name().ok() == Some(component.as_str())),
            "split component must use its embedded dispatcher, not relocate to {:?}",
            target.name().ok()
        );
    }
}

pub(super) fn compile_and_run_with_small_stack(
    object_path: &std::path::Path,
    harness_path: &std::path::Path,
    executable_path: &std::path::Path,
) {
    let compile = Command::new("rustc")
        .arg("--edition=2021")
        .arg(harness_path)
        .arg("-C")
        .arg(format!("link-arg={}", object_path.display()))
        .arg("-o")
        .arg(executable_path)
        .output()
        .expect("compile tail-loop harness");
    assert!(
        compile.status.success(),
        "tail-loop harness failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new("bash")
        .args(["-c", "ulimit -s 128; exec \"$1\"", "terlan-tail-loop"])
        .arg(executable_path)
        .output()
        .expect("run tail-loop harness");
    assert!(
        run.status.success(),
        "deep tail-loop failed on a 128 KiB native stack:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

pub(super) fn compile_and_run_many_with_small_stack(
    object_paths: &[std::path::PathBuf],
    harness_path: &std::path::Path,
    executable_path: &std::path::Path,
) {
    let mut compiler = Command::new("rustc");
    compiler.arg("--edition=2021").arg(harness_path);
    for object_path in object_paths {
        compiler
            .arg("-C")
            .arg(format!("link-arg={}", object_path.display()));
    }
    let compile = compiler
        .arg("-o")
        .arg(executable_path)
        .output()
        .expect("compile split tail-loop harness");
    assert!(
        compile.status.success(),
        "split tail-loop harness failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new("bash")
        .args(["-c", "ulimit -s 128; exec \"$1\"", "terlan-split-tail-loop"])
        .arg(executable_path)
        .output()
        .expect("run split tail-loop harness");
    assert!(
        run.status.success(),
        "split deep tail-loop failed on a 128 KiB native stack:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
