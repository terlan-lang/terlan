#[derive(Debug, Clone, PartialEq, Eq)]
/// Backend-neutral pattern representation in CoreIR.
///
/// Inputs:
/// - Typechecked Terlan pattern data.
///
/// Outputs:
/// - CoreIR pattern payload consumed by proof checks and backend lowering.
///
/// Transformation:
/// - Removes source syntax details while preserving semantic match shape.
pub enum CorePattern {
    Wildcard,
    Var(String),
    Int(i64),
    Float(String),
    Atom(String),
    Tuple(Vec<CorePattern>),
    List(Vec<CorePattern>),
    ListCons {
        head: Box<CorePattern>,
        tail: Box<CorePattern>,
    },
    Map(Vec<CoreMapPatternField>),
    Record {
        name: String,
        fields: Vec<CoreRecordPatternField>,
    },
    Constructor {
        name: String,
        constructor_identity: Option<String>,
        args: Vec<CorePattern>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// CoreIR map-pattern field.
///
/// Inputs:
/// - Checked map-pattern key, required flag, and nested pattern.
///
/// Outputs:
/// - Backend-neutral field payload for map-pattern matching.
///
/// Transformation:
/// - Preserves field metadata and recursively typed pattern data.
pub struct CoreMapPatternField {
    pub key: String,
    pub required: bool,
    pub value: CorePattern,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// CoreIR record-pattern field.
///
/// Inputs:
/// - Checked record-pattern key, required flag, and nested pattern.
///
/// Outputs:
/// - Backend-neutral field payload for record-pattern matching.
///
/// Transformation:
/// - Records field matching requirements without committing to backend record
///   syntax.
pub struct CoreRecordPatternField {
    pub key: String,
    pub required: bool,
    pub value: CorePattern,
}

impl CorePattern {
    /// Renders a typed Core pattern as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed Core pattern from the Lean-covered pattern subset.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the structural Core pattern without using source spans,
    ///   backend syntax, or syntax-output summary text.
    pub(crate) fn contract_text(&self) -> String {
        match self {
            CorePattern::Wildcard => "Wildcard".to_string(),
            CorePattern::Var(name) => format!("Var({name})"),
            CorePattern::Int(value) => format!("Int({value})"),
            CorePattern::Float(value) => format!("Float({value})"),
            CorePattern::Atom(value) => format!("Atom({value})"),
            CorePattern::Tuple(elements) => format!(
                "Tuple({})",
                elements
                    .iter()
                    .map(CorePattern::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CorePattern::List(elements) => format!(
                "List({})",
                elements
                    .iter()
                    .map(CorePattern::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CorePattern::ListCons { head, tail } => {
                format!(
                    "ListCons({}|{})",
                    head.contract_text(),
                    tail.contract_text()
                )
            }
            CorePattern::Map(fields) => format!(
                "Map({})",
                fields
                    .iter()
                    .map(CoreMapPatternField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CorePattern::Record { name, fields } => format!(
                "Record({name};{})",
                fields
                    .iter()
                    .map(CoreRecordPatternField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CorePattern::Constructor {
                name,
                constructor_identity,
                args,
            } => {
                let args = args
                    .iter()
                    .map(CorePattern::contract_text)
                    .collect::<Vec<_>>()
                    .join(",");
                match constructor_identity {
                    Some(identity) => format!("Constructor({name};identity={identity};{args})"),
                    None => format!("Constructor({name};{args})"),
                }
            }
        }
    }
}

impl CoreMapPatternField {
    /// Renders a typed Core map-pattern field as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed Core map-pattern field from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the source key, required/optional map-match operator, and
    ///   recursively rendered value pattern without backend-specific syntax.
    fn contract_text(&self) -> String {
        let operator = if self.required { ":=" } else { "=>" };
        format!("{}{}{}", self.key, operator, self.value.contract_text())
    }
}

impl CoreRecordPatternField {
    /// Renders a typed Core record-pattern field as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed Core record-pattern field from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the field key, source field-match operator, and
    ///   recursively rendered value pattern without backend-specific syntax.
    fn contract_text(&self) -> String {
        let operator = if self.required { "=" } else { "=>" };
        format!("{}{}{}", self.key, operator, self.value.contract_text())
    }
}
