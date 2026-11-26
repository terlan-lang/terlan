use serde_json::Value;

/// Generator-owned checked type used to request a result before source exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CheckedType {
    Unit,
    Bool,
    Int,
    Float,
    String,
    Atom(&'static str),
    Product(Vec<Self>),
    Nominal(&'static str),
    Applied {
        constructor: &'static str,
        arguments: Vec<Self>,
    },
}

impl CheckedType {
    pub(crate) fn option(payload: Self) -> Self {
        Self::Applied {
            constructor: "Option",
            arguments: vec![payload],
        }
    }

    pub(crate) fn result(success: Self, failure: Self) -> Self {
        Self::Applied {
            constructor: "Result",
            arguments: vec![success, failure],
        }
    }

    pub(crate) fn source_name(&self) -> String {
        match self {
            Self::Unit => "Unit".into(),
            Self::Bool => "Bool".into(),
            Self::Int => "Int".into(),
            Self::Float => "Float".into(),
            Self::String => "String".into(),
            Self::Atom(value) => format!("Atom[\"{value}\"]"),
            Self::Product(fields) => format!(
                "{{{}}}",
                fields
                    .iter()
                    .map(Self::source_name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Nominal(name) => (*name).into(),
            Self::Applied {
                constructor,
                arguments,
            } => format!(
                "{constructor}[{}]",
                arguments
                    .iter()
                    .map(Self::source_name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

/// Source and value constructed for one previously requested checked type.
pub(crate) struct GeneratedBody {
    pub(crate) checked_type: CheckedType,
    pub(crate) value: Value,
    pub(crate) source: String,
}

impl GeneratedBody {
    pub(crate) fn for_type(checked_type: CheckedType, value: Value, source: String) -> Self {
        Self {
            checked_type,
            value,
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_checked_type_has_one_canonical_source_name() {
        assert_eq!(CheckedType::Unit.source_name(), "Unit");
        assert_eq!(CheckedType::Bool.source_name(), "Bool");
        assert_eq!(CheckedType::Int.source_name(), "Int");
        assert_eq!(CheckedType::Float.source_name(), "Float");
        assert_eq!(CheckedType::String.source_name(), "String");
        assert_eq!(CheckedType::Atom("ready").source_name(), "Atom[\"ready\"]");
        assert_eq!(
            CheckedType::Product(vec![CheckedType::Int, CheckedType::Bool]).source_name(),
            "{Int, Bool}"
        );
        assert_eq!(CheckedType::Nominal("Evidence").source_name(), "Evidence");
        assert_eq!(
            CheckedType::option(CheckedType::result(CheckedType::Int, CheckedType::String))
                .source_name(),
            "Option[Result[Int, String]]"
        );
    }

    #[test]
    fn generated_body_retains_the_requested_type_and_value() {
        let body = GeneratedBody::for_type(CheckedType::Int, Value::from(7), "7".into());
        assert_eq!(body.checked_type, CheckedType::Int);
        assert_eq!(body.value, Value::from(7));
        assert_eq!(body.source, "7");
    }
}
