use crate::terlan_html::{
    render_template_attribute, template_attribute_slot_kind, TemplateAttributeSlotKind,
    TemplateAttributeValue,
};

use super::{StaticSyntaxRenderError, StaticTemplateValue};

pub(super) fn render_static_template_attribute(
    name: &str,
    value: &StaticTemplateValue,
) -> Result<Option<String>, StaticSyntaxRenderError> {
    if let StaticTemplateValue::Optional(optional) = value {
        return match optional {
            Some(inner) => render_static_template_attribute(name, inner),
            None => Ok(None),
        };
    }

    let typed_value = match template_attribute_slot_kind(name) {
        TemplateAttributeSlotKind::Boolean => match value {
            StaticTemplateValue::Bool(value) => TemplateAttributeValue::Boolean(*value),
            _ => TemplateAttributeValue::Unsupported,
        },
        TemplateAttributeSlotKind::TokenList => match value {
            StaticTemplateValue::Text(text) => TemplateAttributeValue::Scalar(text.clone()),
            StaticTemplateValue::List(values) => TemplateAttributeValue::Tokens(
                values
                    .iter()
                    .map(|value| match value {
                        StaticTemplateValue::Text(text) => Ok(text.clone()),
                        _ => Err(StaticSyntaxRenderError::Invalid(format!(
                            "template token-list attribute `{name}` requires text collection members"
                        ))),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            _ => TemplateAttributeValue::Unsupported,
        },
        TemplateAttributeSlotKind::Url => match value {
            StaticTemplateValue::Text(text) => TemplateAttributeValue::Scalar(text.clone()),
            _ => TemplateAttributeValue::Unsupported,
        },
        TemplateAttributeSlotKind::Scalar => match value {
            StaticTemplateValue::Text(text) => TemplateAttributeValue::Scalar(text.clone()),
            StaticTemplateValue::Int(value) => TemplateAttributeValue::Scalar(value.to_string()),
            StaticTemplateValue::Bool(value) => TemplateAttributeValue::Scalar(value.to_string()),
            _ => TemplateAttributeValue::Unsupported,
        },
    };

    render_template_attribute(name, typed_value)
        .map_err(|error| StaticSyntaxRenderError::Invalid(error.into()))
}
