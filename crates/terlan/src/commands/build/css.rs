//! Typed stylesheet evaluation and a private Lightning CSS bundling adapter.

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use std::sync::OnceLock;

use std::process::Command;
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use terlan_runtime_abi::{DescriptorValue, DescriptorView};

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use crate::runtime::vm::ReplValue;

use super::write_build_file;

/// Bundles manifest styles in declared order. Terlan files export stylesheet/0;
/// CSS files are imported directly. Generated files stay in the build directory.
pub(super) fn bundle_stylesheets(
    sources: &[PathBuf],
    build_root: &Path,
    incremental: bool,
) -> Result<Option<PathBuf>, String> {
    if sources.is_empty() {
        return Ok(None);
    }
    let directory = build_root.join("terlan-css");
    fs::create_dir_all(&directory).map_err(|error| format!("error[css_build]: {error}"))?;
    let directory = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let workspace = directory.join("evaluation");
    let mut imports = String::new();
    for (index, source) in sources.iter().enumerate() {
        let source = source.canonicalize().map_err(|error| {
            format!(
                "error[css_source]: cannot read {}: {error}",
                source.display()
            )
        })?;
        let path = match source.extension().and_then(|value| value.to_str()) {
            Some("css") => source,
            Some("terl") => {
                let css = evaluate_stylesheet(&source, &workspace)
                    .map_err(|error| format!("error[css_source]: {}: {error}", source.display()))?;
                let path = directory.join(format!("source-{index}.css"));
                write_build_file(&path, css.as_bytes(), incremental)?;
                path
            }
            _ => {
                return Err(format!(
                    "error[css_source]: expected .css or .terl: {}",
                    source.display()
                ))
            }
        };
        imports.push_str("@import ");
        cssparser::serialize_string(&path.to_string_lossy(), &mut imports)
            .map_err(|error| error.to_string())?;
        imports.push_str(";\n");
    }
    let entry = directory.join("entry.css");
    write_build_file(&entry, imports.as_bytes(), incremental)?;
    let output = directory.join("styles.css");
    let toolchain = super::web_toolchain::resolve_managed_web_toolchain()?;
    let result = super::web_toolchain::run_managed_bundler(
        Command::new("node")
            .arg(toolchain.root.join("css-bundle.mjs"))
            .arg(&entry)
            .arg(&output),
    )
    .map_err(|error| format!("error[css_bundle]: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "error[css_bundle]: {}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    Ok(Some(output))
}

fn evaluate_stylesheet(source: &Path, workspace: &Path) -> Result<String, String> {
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    {
        super::vm_artifact::source_exports::with_source_exports(
            source,
            workspace,
            &["stylesheet"],
            |_, values, _| render_stylesheet(&values[0]),
        )
    }
    #[cfg(all(not(test), feature = "serve-runtime-bin"))]
    {
        let _ = (source, workspace);
        Err("stylesheet source compilation requires the compiler executable".into())
    }
}

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn property_names() -> &'static BTreeMap<String, String> {
    static NAMES: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let data: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../std/css/bindings.json"
        )))
        .expect("generated CSS binding manifest must be valid JSON");
        data["properties"]
            .as_array()
            .expect("CSS properties array")
            .iter()
            .map(|property| {
                (
                    property["field"].as_str().unwrap().to_string(),
                    property["name"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    })
}

/// Projects typed records after native evaluation; no source-text interpretation
/// or Terlan-specific style control flow is introduced.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn render_stylesheet(value: &ReplValue) -> Result<String, String> {
    let value = match value.descriptor_view() {
        DescriptorView::Record(name, fields)
            if name.rsplit('.').next() == Some("Ok") && fields.len() == 1 =>
        {
            &fields
                .iter()
                .find(|(name, _)| name == "value")
                .ok_or("invalid stylesheet Ok result")?
                .1
        }
        DescriptorView::Record(name, fields)
            if name.rsplit('.').next() == Some("Err") && fields.len() == 1 =>
        {
            return Err(format!("stylesheet returned an error: {:?}", fields[0].1));
        }
        DescriptorView::Tuple(values) if values.len() == 2 => {
            match values[0].descriptor_view() {
                DescriptorView::Atom("ok") => &values[1],
                DescriptorView::Atom("error") => {
                    return Err(format!("stylesheet returned an error: {:?}", values[1]))
                }
                _ => return Err(
                    "stylesheet must return a list of Rule values or Result containing that list"
                        .into(),
                ),
            }
        }
        _ => value,
    };
    let DescriptorView::List(rules) = value.descriptor_view() else {
        return Err(format!(
            "stylesheet must return a list of Rule values; found {value:?}"
        ));
    };
    let mut output = String::new();
    for rule in rules {
        let DescriptorView::Tuple(fields) = rule.descriptor_view() else {
            return Err("stylesheet entry must be a tagged Rule value".into());
        };
        let [tag, selector, style] = fields else {
            return Err("stylesheet entry must have the Rule selector/style shape".into());
        };
        if !matches!(tag.descriptor_view(), DescriptorView::Atom("css_rule")) {
            return Err("stylesheet entry must have the css_rule tag".into());
        }
        let DescriptorView::String(selector) = selector.descriptor_view() else {
            return Err("CSS selector must be a String".into());
        };
        if selector.trim().is_empty() || selector.contains(['{', '}', ';']) {
            return Err(format!("invalid CSS selector: {selector:?}"));
        }
        let DescriptorView::Record(_, declarations) = style.descriptor_view() else {
            return Err("CSS style must be a struct of typed properties".into());
        };
        output.push_str(selector);
        output.push_str(" {\n");
        let mut seen = BTreeSet::new();
        for (field, value) in declarations {
            let property = property_names()
                .get(field)
                .ok_or_else(|| format!("unknown CSS property field `{field}`"))?;
            if !seen.insert(field) {
                return Err(format!("duplicate CSS property `{property}`"));
            }
            let value = match value.descriptor_view() {
                DescriptorView::Atom("none") => continue,
                DescriptorView::Record(name, fields)
                    if name.rsplit('.').next() == Some("None") && fields.is_empty() =>
                {
                    continue
                }
                DescriptorView::Record(name, fields)
                    if name.rsplit('.').next() == Some("Some") && fields.len() == 1 =>
                {
                    &fields
                        .iter()
                        .find(|(name, _)| name == "value")
                        .ok_or("invalid optional CSS value")?
                        .1
                }
                DescriptorView::Tuple(values)
                    if values.len() == 2
                        && matches!(values[0].descriptor_view(), DescriptorView::Atom("some")) =>
                {
                    &values[1]
                }
                _ => value,
            };
            let DescriptorView::Record(_, token) = value.descriptor_view() else {
                return Err(format!(
                    "CSS property `{property}` must use its generated Value type"
                ));
            };
            let identity = token
                .iter()
                .find(|(name, _)| name == "property")
                .map(|(_, value)| value.descriptor_view());
            if token.len() != 2
                || !matches!(identity, Some(DescriptorView::String(name)) if name == property)
            {
                return Err(format!(
                    "CSS value belongs to a different property than `{property}`"
                ));
            }
            let text = token
                .iter()
                .find(|(name, _)| name == "css")
                .map(|(_, value)| value.descriptor_view());
            let Some(DescriptorView::String(text)) = text else {
                return Err(format!(
                    "CSS property `{property}` has an invalid value descriptor"
                ));
            };
            if text.is_empty() || text.contains([';', '{', '}']) {
                return Err(format!("invalid CSS value for `{property}`"));
            }
            output.push_str("  ");
            output.push_str(property);
            output.push_str(": ");
            output.push_str(text);
            output.push_str(";\n");
        }
        output.push_str("}\n");
    }
    Ok(output)
}

#[cfg(test)]
#[path = "css_test.rs"]
mod tests;
