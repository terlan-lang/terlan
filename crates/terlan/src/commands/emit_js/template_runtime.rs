use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::commands::artifacts::collect_syntax_template_frontend_inputs;
use crate::terlan_html::{
    template_attribute_slot_kind, HtmlAttrValue, HtmlNode, HtmlSlot, TemplateAttributeSlotKind,
};
use crate::terlan_syntax::{SyntaxModuleOutput, SyntaxTemplatePropOutput};
use crate::terlan_typeck::{core_expr_lowering::core_expr_from_syntax, CoreExpr};

use super::direct_helpers::is_direct_oxc_js_identifier;

const TEMPLATE_RUNTIME_SOURCE: &str = r#"
function __terlanTemplateEscapeText(value) {
  return String(value).replace(/[<>"'`\/&= \t\n\f\r\0]/g, (character) => {
    switch (character) {
      case "<": return "&lt;";
      case ">": return "&gt;";
      case "\"": return "&quot;";
      case "'": return "&apos;";
      case "`": return "&grave;";
      case "/": return "&#47;";
      case "&": return "&amp;";
      case "=": return "&#61;";
      case " ": return "&#32;";
      case "\t": return "&#9;";
      case "\n": return "&#10;";
      case "\f": return "&#12;";
      case "\r": return "&#13;";
      case "\0": return "&#65533;";
    }
  });
}

function __terlanTemplateEscapeAttribute(value) {
  return String(value).replace(/[&"<>]/g, (character) => ({
    "&": "&amp;", "\"": "&quot;", "<": "&lt;", ">": "&gt;"
  })[character]);
}

function __terlanTemplateSlot(props, path, label) {
  let value = props;
  for (const field of path) {
    if (value === null || value === undefined || !Object.hasOwn(value, field)) {
      throw new TypeError(`missing template slot value \`${label}\``);
    }
    value = value[field];
  }
  return value;
}

function __terlanTemplateOptional(value) {
  if (Array.isArray(value) && value[0] === "Some" && value.length === 2) return value[1];
  if (Array.isArray(value) && value[0] === "None" && value.length === 1) return undefined;
  return value;
}

function __terlanTemplateTrusted(value, label) {
  if (Array.isArray(value) && value[0] === "Html" && value.length === 2 && typeof value[1] === "string") {
    return value[1];
  }
  throw new TypeError(`template trusted slot \`${label}\` requires Template.Html`);
}

function __terlanTemplateText(value, trusted, label) {
  value = __terlanTemplateOptional(value);
  if (trusted) return __terlanTemplateTrusted(value, label);
  if (typeof value !== "string" && typeof value !== "number" && typeof value !== "boolean") {
    throw new TypeError(`template text slot \`${label}\` requires a scalar value`);
  }
  return __terlanTemplateEscapeText(value);
}

function __terlanTemplateUnsafeUrl(name) {
  return `template URL attribute \`${name}\` rejects an unsafe URL`;
}

function __terlanTemplateAttribute(name, kind, value) {
  value = __terlanTemplateOptional(value);
  if (value === null || value === undefined) return "";
  if (kind === "boolean") {
    if (typeof value !== "boolean") {
      throw new TypeError(`template boolean attribute \`${name}\` requires a Bool value`);
    }
    return value ? ` ${name}` : "";
  }
  if (kind === "tokens") {
    if (typeof value === "string") {
      return ` ${name}=\"${__terlanTemplateEscapeAttribute(value)}\"`;
    }
    if (!Array.isArray(value)) {
      throw new TypeError(`template token-list attribute \`${name}\` requires text or a text collection`);
    }
    value.forEach((token, index) => {
      if (typeof token !== "string" || token.length === 0 || /\s/u.test(token)) {
        throw new TypeError(`template token-list attribute \`${name}\` has invalid token at index ${index}`);
      }
    });
    return ` ${name}=\"${__terlanTemplateEscapeAttribute(value.join(" "))}\"`;
  }
  if (kind === "url") {
    if (typeof value !== "string") {
      throw new TypeError(`template URL attribute \`${name}\` requires a URL text value`);
    }
    if (value.trim() !== value || /[\u0000-\u001f\u007f]/u.test(value)) {
      throw new TypeError(__terlanTemplateUnsafeUrl(name));
    }
    let parsed;
    try {
      parsed = new URL(value, "https://template.invalid/");
    } catch (_error) {
      throw new TypeError(__terlanTemplateUnsafeUrl(name));
    }
    if (!["http:", "https:", "mailto:", "tel:"].includes(parsed.protocol)) {
      throw new TypeError(__terlanTemplateUnsafeUrl(name));
    }
  } else if (!["string", "number", "boolean"].includes(typeof value)) {
    throw new TypeError(`template attribute \`${name}\` requires a scalar value`);
  }
  return ` ${name}=\"${__terlanTemplateEscapeAttribute(value)}\"`;
}

function __terlanTemplateComponentProps(attributes, children, props) {
  const componentProps = {};
  for (const attribute of attributes) {
    if (attribute[0] === "present") componentProps[attribute[1]] = true;
    else if (attribute[0] === "text") componentProps[attribute[1]] = attribute[2];
    else componentProps[attribute[1]] = __terlanTemplateOptional(
      __terlanTemplateSlot(props, attribute[3], attribute[4])
    );
  }
  componentProps.children = ["Html", children];
  return componentProps;
}

function __terlanRenderTemplateNodes(nodes, props) {
  let output = "";
  for (const node of nodes) {
    if (node[0] === "text") output += node[1];
    else if (node[0] === "comment") output += `<!--${node[1]}-->`;
    else if (node[0] === "doctype") output += `<!DOCTYPE ${node[1]}>`;
    else if (node[0] === "slot") {
      output += __terlanTemplateText(__terlanTemplateSlot(props, node[1], node[3]), node[2], node[3]);
    } else if (node[0] === "component") {
      const renderer = __terlanTemplateRenderers[node[1]];
      if (typeof renderer !== "function") {
        throw new TypeError(`template component \`${node[1]}\` has no generated renderer`);
      }
      const children = __terlanRenderTemplateNodes(node[3], props);
      output += renderer(__terlanTemplateComponentProps(node[2], children, props));
    } else if (node[0] === "element") {
      output += `<${node[1]}`;
      for (const attribute of node[2]) {
        if (attribute[0] === "present") output += ` ${attribute[1]}`;
        else if (attribute[0] === "text") {
          output += ` ${attribute[1]}=\"${__terlanTemplateEscapeAttribute(attribute[2])}\"`;
        } else {
          output += __terlanTemplateAttribute(
            attribute[1], attribute[2], __terlanTemplateSlot(props, attribute[3], attribute[4])
          );
        }
      }
      output += `>${__terlanRenderTemplateNodes(node[3], props)}</${node[1]}>`;
    }
  }
  return output;
}
"#;

/// Returns the private JavaScript renderer name for a Terlan template.
pub(super) fn template_renderer_identifier(name: &str) -> Option<String> {
    let identifier = format!("__terlan_template_{name}");
    is_direct_oxc_js_identifier(&identifier).then_some(identifier)
}

/// Emits the browser/shared-JS runtime and descriptors for external templates.
pub(super) fn emit_template_runtime(
    module: &SyntaxModuleOutput,
    source_path: &Path,
) -> Result<String, String> {
    let collected = collect_syntax_template_frontend_inputs(module, source_path);
    if !collected.errors.is_empty() {
        return Err(collected
            .errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
            .join("\n"));
    }
    if collected.inputs.is_empty() {
        return Ok(String::new());
    }

    let inputs = collected.inputs;
    let component_tags = inputs
        .iter()
        .filter_map(|input| {
            input
                .parsed
                .tag_name
                .as_ref()
                .map(|tag| (tag.clone(), input.name.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let mut output = TEMPLATE_RUNTIME_SOURCE.to_string();
    for input in &inputs {
        let renderer = template_renderer_identifier(&input.name)
            .ok_or_else(|| format!("template `{}` cannot be emitted as JavaScript", input.name))?;
        let prop_types = input
            .props
            .iter()
            .map(|prop| (prop.name.as_str(), prop))
            .collect::<BTreeMap<_, _>>();
        let expressions = template_expression_js(&input.parsed.nodes)?;
        let expression_keys = expressions
            .iter()
            .enumerate()
            .map(|(index, (source, _))| (source.clone(), format!("__terlan_expr_{index}")))
            .collect::<BTreeMap<_, _>>();
        let descriptor = template_nodes_descriptor(
            &input.parsed.nodes,
            &prop_types,
            &expression_keys,
            &component_tags,
        );
        let descriptor_name = format!("{renderer}_nodes");
        let prop_bindings = input
            .props
            .iter()
            .map(|prop| prop.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let expression_bindings = expressions
            .iter()
            .map(|(source, js)| {
                let key = &expression_keys[source];
                format!("{key}: ({js})")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let render_props = if expression_bindings.is_empty() {
            "props".to_string()
        } else {
            format!("{{ ...props, {expression_bindings} }}")
        };
        let prop_prelude = if prop_bindings.is_empty() || expressions.is_empty() {
            String::new()
        } else {
            format!(" const {{ {prop_bindings} }} = props;")
        };
        output.push_str(&format!(
            "\nconst {descriptor_name} = {};\nfunction {renderer}(props) {{{prop_prelude} return __terlanRenderTemplateNodes({descriptor_name}, {render_props}); }}\n",
            serde_json::to_string(&descriptor)
                .map_err(|error| format!("failed to encode template `{}`: {error}", input.name))?
        ));
    }
    let renderers = inputs
        .iter()
        .map(|input| {
            let renderer = template_renderer_identifier(&input.name)
                .expect("template renderer identifier was validated above");
            format!("{}: {renderer}", input.name)
        })
        .collect::<Vec<_>>()
        .join(", ");
    output.push_str(&format!(
        "\nconst __terlanTemplateRenderers = {{ {renderers} }};\n"
    ));
    Ok(output)
}

/// Emits browser-callable renderers for imported JSON/XML/YAML/TOML/text
/// templates. The renderer source is produced by the same descriptor builder
/// used by static and VM artifact rendering.
pub(super) fn emit_structured_artifact_runtime(
    module: &SyntaxModuleOutput,
    source_path: &Path,
) -> Result<String, String> {
    let imports = crate::commands::artifacts::collect_syntax_asset_imports(module, source_path)?;
    let mut output = String::new();
    for import in imports {
        let Some(target) =
            crate::terlan_html::artifact_template_target_from_path(&import.resolved_path)
        else {
            continue;
        };
        if target.parses_to_html_tree() {
            continue;
        }
        let source = std::str::from_utf8(&import.bytes).map_err(|error| {
            format!(
                "error[template_backend_encoding]: {}: {error}",
                import.resolved_path.display()
            )
        })?;
        let export_name = format!("render_artifact_{}", import.alias);
        output.push_str(
            &crate::terlan_html::emit_structured_template_browser_renderer(
                source,
                &import.resolved_path,
                &export_name,
            )?,
        );
        output.push('\n');
    }
    Ok(output)
}

fn template_nodes_descriptor(
    nodes: &[HtmlNode],
    props: &BTreeMap<&str, &SyntaxTemplatePropOutput>,
    expressions: &BTreeMap<String, String>,
    component_tags: &BTreeMap<String, String>,
) -> Value {
    Value::Array(
        nodes
            .iter()
            .map(|node| template_node_descriptor(node, props, expressions, component_tags))
            .collect(),
    )
}

fn template_node_descriptor(
    node: &HtmlNode,
    props: &BTreeMap<&str, &SyntaxTemplatePropOutput>,
    expressions: &BTreeMap<String, String>,
    component_tags: &BTreeMap<String, String>,
) -> Value {
    match node {
        HtmlNode::Text(text) => json!(["text", text]),
        HtmlNode::Comment(text) => json!(["comment", text]),
        HtmlNode::Doctype(text) => json!(["doctype", text]),
        HtmlNode::Slot(slot) => json!([
            "slot",
            template_slot_runtime_path(slot, expressions),
            template_slot_is_trusted(slot, props),
            slot.expression
        ]),
        HtmlNode::Element(element) => {
            let attributes = element
                .attrs
                .iter()
                .map(|attribute| match &attribute.value {
                    None => json!(["present", attribute.name]),
                    Some(HtmlAttrValue::Text(text)) => json!(["text", attribute.name, text]),
                    Some(HtmlAttrValue::Slot(slot)) => json!([
                        "slot",
                        attribute.name,
                        attribute_kind_name(template_attribute_slot_kind(&attribute.name)),
                        template_slot_runtime_path(slot, expressions),
                        slot.expression
                    ]),
                })
                .collect::<Vec<_>>();
            if let Some(component_name) = component_tags.get(&element.name) {
                return json!([
                    "component",
                    component_name,
                    attributes,
                    template_nodes_descriptor(
                        &element.children,
                        props,
                        expressions,
                        component_tags
                    )
                ]);
            }
            json!([
                "element",
                element.name,
                attributes,
                template_nodes_descriptor(&element.children, props, expressions, component_tags)
            ])
        }
    }
}

fn template_slot_runtime_path(
    slot: &HtmlSlot,
    expressions: &BTreeMap<String, String>,
) -> Vec<String> {
    if slot.path.is_empty() {
        expressions
            .get(&slot.expression)
            .cloned()
            .into_iter()
            .collect()
    } else {
        slot.path.clone()
    }
}

fn template_expression_js(nodes: &[HtmlNode]) -> Result<BTreeMap<String, String>, String> {
    let mut sources = Vec::new();
    collect_template_expression_sources(nodes, &mut sources);
    let mut expressions = BTreeMap::new();
    for source in sources {
        if expressions.contains_key(&source) {
            continue;
        }
        let syntax =
            crate::terlan_syntax::parse_expr_as_syntax_output(&source).map_err(|error| {
                format!("template expression `{source}` failed to parse for JavaScript: {error:?}")
            })?;
        let core: CoreExpr = core_expr_from_syntax(&syntax).ok_or_else(|| {
            format!("template expression `{source}` has no CoreIR JavaScript lowering")
        })?;
        let js = super::core_lowering::core_expr_to_js(&core).ok_or_else(|| {
            format!("template expression `{source}` is unsupported by JavaScript lowering")
        })?;
        expressions.insert(source, js);
    }
    Ok(expressions)
}

fn collect_template_expression_sources(nodes: &[HtmlNode], sources: &mut Vec<String>) {
    for node in nodes {
        match node {
            HtmlNode::Slot(slot) if slot.path.is_empty() => sources.push(slot.expression.clone()),
            HtmlNode::Element(element) => {
                for attr in &element.attrs {
                    if let Some(HtmlAttrValue::Slot(slot)) = &attr.value {
                        if slot.path.is_empty() {
                            sources.push(slot.expression.clone());
                        }
                    }
                }
                collect_template_expression_sources(&element.children, sources);
            }
            HtmlNode::Text(_) | HtmlNode::Comment(_) | HtmlNode::Doctype(_) | HtmlNode::Slot(_) => {
            }
        }
    }
}

fn template_slot_is_trusted(
    slot: &HtmlSlot,
    props: &BTreeMap<&str, &SyntaxTemplatePropOutput>,
) -> bool {
    let Some(root) = slot.path.first() else {
        return false;
    };
    root == "children"
        || props.get(root.as_str()).is_some_and(|prop| {
            let ty = prop.annotation.text.trim();
            ty == "Html" || ty.ends_with(".Html")
        })
}

fn attribute_kind_name(kind: TemplateAttributeSlotKind) -> &'static str {
    match kind {
        TemplateAttributeSlotKind::Scalar => "scalar",
        TemplateAttributeSlotKind::Url => "url",
        TemplateAttributeSlotKind::Boolean => "boolean",
        TemplateAttributeSlotKind::TokenList => "tokens",
    }
}

#[cfg(test)]
#[path = "template_runtime_test.rs"]
#[cfg(test)]
mod template_runtime_test;
