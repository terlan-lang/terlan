//! Checked construction and mutation of application-owned TypeScript data records.

use std::collections::BTreeSet;

use super::ts_dom_module_mapping::{DomMemberPlan, DomModulePlan, DomPropertyPlan};
use serde_json::{json, Value};

pub(super) struct RecordOperation {
    name: String,
    receiver: Option<String>,
    type_parameters: Vec<String>,
    fields: Vec<DomPropertyPlan>,
    result: String,
}

/// Getter-only DOM contracts stay unchanged. Direct declaration bindings gain
/// setters for supported writable fields and constructors for complete records.
pub(super) fn operations(module: &DomModulePlan) -> Vec<RecordOperation> {
    if module.alias_target.is_some() {
        return Vec::new();
    }
    let mut names = module
        .members
        .iter()
        .map(|member| match member {
            DomMemberPlan::Property(property) => property.terlan_name.clone(),
            DomMemberPlan::Method(method) => method.terlan_name.clone(),
        })
        .collect::<BTreeSet<_>>();
    let reference = format!("{}{}", module.type_name, generics(&module.type_params));
    let properties = module
        .members
        .iter()
        .filter_map(|member| match member {
            DomMemberPlan::Property(property) => Some(property),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut operations = Vec::new();
    if module.constructible {
        operations.push(RecordOperation {
            name: unique("construct", &mut names),
            receiver: None,
            type_parameters: module.type_params.clone(),
            fields: properties
                .iter()
                .map(|property| (*property).clone())
                .collect(),
            result: reference.clone(),
        });
    }
    for property in properties {
        if !property.readonly && property.write_supported {
            operations.push(RecordOperation {
                name: unique(&format!("set_{}", property.terlan_name), &mut names),
                receiver: Some(reference.clone()),
                type_parameters: Vec::new(),
                fields: vec![property.clone()],
                result: "std.core.Unit.Unit".into(),
            });
        }
    }
    operations
}

impl RecordOperation {
    fn arguments(&self) -> Vec<(String, String)> {
        if let Some(receiver) = &self.receiver {
            vec![
                ("receiver".into(), receiver.clone()),
                ("value".into(), self.fields[0].terlan_type.clone()),
            ]
        } else {
            self.fields
                .iter()
                .map(|field| (field.terlan_name.clone(), field.terlan_type.clone()))
                .collect()
        }
    }

    pub(super) fn source(&self, body: bool) -> String {
        let arguments = self.arguments();
        let parameters = arguments
            .iter()
            .skip(usize::from(self.receiver.is_some()))
            .map(|(name, ty)| format!("{name}: {ty}"))
            .collect::<Vec<_>>()
            .join(", ");
        let receiver = self
            .receiver
            .as_ref()
            .map(|ty| format!("(receiver: {ty}) "))
            .unwrap_or_default();
        let ending = if body { " ->\n    native." } else { "." };
        format!(
            "pub {receiver}{}{}({parameters}): {}{ending}\n",
            self.name,
            generics(&self.type_parameters),
            self.result
        )
    }

    pub(super) fn manifest(&self) -> Value {
        json!({
            "name": self.name,
            "js_name": self.fields.first().map(|field| field.js_name.as_str()).unwrap_or(""),
            "kind": if self.receiver.is_some() { "setter" } else { "record" },
            "arity": self.fields.len() + usize::from(self.receiver.is_some()),
            "return_type": self.result,
            "js_names": self.fields.iter().map(|field| &field.js_name).collect::<Vec<_>>(),
            "parameters": self.fields.iter().map(|field| json!({
                "optional": false,
                "undefined_only": field.optional || field.undefined_only,
                "type": field.terlan_type,
            })).collect::<Vec<_>>(),
        })
    }

    pub(super) fn signature_test(&self, module: &str) -> String {
        let arguments = self.arguments();
        let parameters = arguments
            .iter()
            .map(|(name, ty)| format!("{name}: {ty}"))
            .collect::<Vec<_>>()
            .join(", ");
        let values = arguments
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "pub {}_typechecks{}({parameters}): {} ->\n    {module}.{}({values}).\n",
            self.name,
            generics(&self.type_parameters),
            self.result,
            self.name
        )
    }
}

fn generics(parameters: &[String]) -> String {
    if parameters.is_empty() {
        String::new()
    } else {
        format!("[{}]", parameters.join(", "))
    }
}

fn unique(base: &str, names: &mut BTreeSet<String>) -> String {
    if names.insert(base.into()) {
        return base.into();
    }
    for suffix in 2.. {
        let candidate = format!("{base}_{suffix}");
        if names.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!()
}
