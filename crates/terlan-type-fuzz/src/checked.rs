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

    pub(crate) fn list(element: Self) -> Self {
        Self::Applied {
            constructor: "List",
            arguments: vec![element],
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
    prelude: String,
    expression: String,
}

impl GeneratedBody {
    pub(crate) fn expression(checked_type: CheckedType, value: Value, expression: String) -> Self {
        Self {
            checked_type,
            value,
            prelude: String::new(),
            expression,
        }
    }

    pub(crate) fn with_prelude(mut self, prelude: impl Into<String>) -> Self {
        self.prelude = prelude.into();
        self
    }

    pub(crate) fn source(&self) -> String {
        let signature = format!("pub sample(): {} ->", self.checked_type.source_name());
        let expression = self
            .expression
            .lines()
            .map(|line| format!("    {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        if self.prelude.is_empty() {
            format!("{signature}\n{expression}.")
        } else {
            format!("{}\n\n{signature}\n{expression}.", self.prelude)
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
        assert_eq!(
            CheckedType::list(CheckedType::Float).source_name(),
            "List[Float]"
        );
    }

    #[test]
    fn generated_body_retains_the_requested_type_and_value() {
        let body = GeneratedBody::expression(CheckedType::Int, Value::from(7), "7".into());
        assert_eq!(body.checked_type, CheckedType::Int);
        assert_eq!(body.value, Value::from(7));
        assert_eq!(body.source(), "pub sample(): Int ->\n    7.");
        assert_eq!(
            body.with_prelude("pub type Number = Int.").source(),
            "pub type Number = Int.\n\npub sample(): Int ->\n    7."
        );
    }
}
