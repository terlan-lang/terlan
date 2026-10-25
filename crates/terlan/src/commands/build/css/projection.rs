//! Projects typed CSS values after native evaluation.
use crate::runtime::vm::ReplValue;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use terlan_runtime_abi::{DescriptorValue, DescriptorView};

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
#[cfg(test)]
fn render_stylesheet(value: &ReplValue) -> Result<String, String> {
    render_stylesheet_with_options(value, &BTreeSet::new())
}

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
pub(super) fn render_stylesheet_with_options(
    value: &ReplValue,
    optional_fields: &BTreeSet<(String, String)>,
) -> Result<String, String> {
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
        let DescriptorView::Record(style_name, declarations) = style.descriptor_view() else {
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
                DescriptorView::Atom("none")
                    if optional_fields.contains(&(style_name.to_string(), field.to_string())) =>
                {
                    continue
                }
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
            let text = declaration_css(property, value)?;
            if text.is_empty() || text.contains([';', '{', '}']) {
                return Err(format!("invalid CSS value for `{property}`"));
            }
            output.push_str("  ");
            output.push_str(property);
            output.push_str(": ");
            output.push_str(&text);
            output.push_str(";\n");
        }
        output.push_str("}\n");
    }
    Ok(output)
}

#[cfg(test)]
#[path = "../css_test.rs"]
mod tests;

/// Optionality is a checked field-type property; the CSS keyword none is data.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
pub(super) fn optional_style_fields(
    interfaces: &[crate::terlan_hir::ModuleInterface],
) -> BTreeSet<(String, String)> {
    use crate::terlan_typeck::{core_type_from_text, CoreType};
    fn is_option(ty: &CoreType) -> bool {
        matches!(ty, CoreType::Apply {constructor, ..} if constructor.rsplit('.').next() == Some("Option"))
    }
    let mut result = BTreeSet::new();
    for interface in interfaces {
        for (name, fields) in &interface.struct_fields {
            for field in fields {
                if core_type_from_text(&field.annotation).is_some_and(|ty| is_option(&ty)) {
                    for record in [name.clone(), format!("{}.{}", interface.module, name)] {
                        result.insert((record, field.name.clone()));
                    }
                }
            }
        }
    }
    result
}

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn property_definition(property: &str) -> &'static serde_json::Value {
    static DEFINITIONS: OnceLock<serde_json::Value> = OnceLock::new();
    let data = DEFINITIONS.get_or_init(|| {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../std/css/bindings.json"
        )))
        .expect("CSS bindings")
    });
    data["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["name"] == property)
        .expect("known CSS property")
}

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn declaration_css(property: &str, value: &ReplValue) -> Result<String, String> {
    let primitive_allowed = |kind: &str| {
        let entry = property_definition(property);
        let mut atoms = entry["constructors"]
            .as_array()
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        atoms.extend(
            entry["repetition"]["atoms"]
                .as_array()
                .into_iter()
                .flatten(),
        );
        if let Some(groups) = entry["composition"]["groups"].as_array() {
            for group in groups {
                atoms.extend(group["atoms"].as_array().into_iter().flatten());
            }
        }
        atoms
            .iter()
            .any(|atom| atom["kind"] == "primitive" && atom["name"] == kind)
    };
    let keyword_allowed = |keyword: &str| {
        let entry = property_definition(property);
        let mut atoms = entry["constructors"]
            .as_array()
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        atoms.extend(
            entry["repetition"]["atoms"]
                .as_array()
                .into_iter()
                .flatten(),
        );
        if let Some(groups) = entry["composition"]["groups"].as_array() {
            for group in groups {
                atoms.extend(group["atoms"].as_array().into_iter().flatten());
            }
        }
        ["inherit", "initial", "unset", "revert", "revert-layer"].contains(&keyword)
            || atoms
                .iter()
                .any(|atom| atom["kind"] == "keyword" && atom["name"] == keyword)
            || (primitive_allowed("color")
                && ["transparent", "currentColor", "currentcolor"].contains(&keyword))
    };
    if let ReplValue::Float(number) = value {
        let number = number.parse::<f64>().map_err(|_| "invalid CSS number")?;
        return if primitive_allowed("number") && number.is_finite() {
            Ok(number.to_string())
        } else {
            Err(format!("unsupported numeric value for `{property}`"))
        };
    }
    match value.descriptor_view() {
        DescriptorView::Atom(keyword) if keyword_allowed(keyword) => Ok(keyword.to_string()),
        DescriptorView::Int(number)
            if primitive_allowed("integer") || primitive_allowed("number") =>
        {
            Ok(number.to_string())
        }
        DescriptorView::String(text) if primitive_allowed("color") => Ok(text.to_string()),
        DescriptorView::Tuple(fields)
            if fields.len() == 2
                && matches!(
                    fields[0].descriptor_view(),
                    DescriptorView::Atom("css_parts")
                ) =>
        {
            let parts = match fields[1].descriptor_view() {
                DescriptorView::List(parts) | DescriptorView::Tuple(parts) => parts,
                _ => return Err("invalid CSS composition parts".into()),
            };
            parts
                .iter()
                .map(|part| declaration_css(property, part))
                .collect::<Result<Vec<_>, _>>()
                .map(|parts| parts.join(" "))
        }
        DescriptorView::Record(name, fields)
            if name.rsplit('.').next() == Some("Color") && primitive_allowed("color") =>
        {
            let text = fields
                .iter()
                .find(|(name, _)| name.trim_start_matches('#') == "css")
                .ok_or("color has no CSS text")?;
            let DescriptorView::String(text) = text.1.descriptor_view() else {
                return Err("color text must be String".into());
            };
            Ok(text.to_string())
        }
        DescriptorView::Record(name, fields)
            if ["Length", "Percentage", "Angle", "Time"]
                .contains(&name.rsplit('.').next().unwrap_or(name)) =>
        {
            let kind = name.rsplit('.').next().unwrap_or(name).to_lowercase();
            if !primitive_allowed(&kind) {
                return Err(format!("{kind} is not allowed for {property}"));
            }
            let amount = fields
                .iter()
                .find(|(name, _)| name.trim_start_matches('#') == "amount")
                .ok_or("CSS unit has no amount")?;
            let number = match &amount.1 {
                ReplValue::Float(n) => n
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .ok_or("CSS amount must be finite")?
                    .to_string(),
                ReplValue::Int(n) => n.to_string(),
                _ => return Err("CSS amount must be finite".into()),
            };
            let unit = if kind == "percentage" {
                "%"
            } else {
                let unit = fields
                    .iter()
                    .find(|(name, _)| name.trim_start_matches('#') == "unit")
                    .ok_or("CSS value has no unit")?;
                let DescriptorView::Atom(unit) = unit.1.descriptor_view() else {
                    return Err("CSS unit must be an enum".into());
                };
                match (kind.as_str(), unit) {
                    ("length", "px" | "rem" | "em" | "vw" | "vh")
                    | ("angle", "deg")
                    | ("time", "ms" | "s") => unit,
                    _ => return Err("invalid CSS unit".into()),
                }
            };
            Ok(format!("{number}{unit}"))
        }
        DescriptorView::Record(_, fields) => {
            let identity = fields
                .iter()
                .find(|(name, _)| name == "property")
                .map(|(_, value)| value.descriptor_view());
            let text = fields
                .iter()
                .find(|(name, _)| name == "css")
                .map(|(_, value)| value.descriptor_view());
            if fields.len() != 2
                || !matches!(identity, Some(DescriptorView::String(name)) if name == property)
            {
                return Err(format!(
                    "CSS value belongs to a different property than `{property}`"
                ));
            }
            match text {
                Some(DescriptorView::String(text)) => Ok(text.to_string()),
                _ => Err("invalid checked CSS value".into()),
            }
        }
        _ => Err(format!("unsupported value for CSS property `{property}`")),
    }
}
