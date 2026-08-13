#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    String(String),
    StringPattern(Vec<CoreStringPatternSegment>),
    Atom(String),
    Tuple(Vec<CorePattern>),
    Alias {
        alias: String,
        pattern: Box<CorePattern>,
    },
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
    BinaryLayout {
        endian: CoreBinaryPatternEndian,
        fields: Vec<CoreBinaryPatternField>,
    },
    Constructor {
        name: String,
        constructor_identity: Option<String>,
        args: Vec<CorePattern>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Variants representing core binary pattern endian.
pub enum CoreBinaryPatternEndian {
    Big,
    Little,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Variants representing core binary pattern descriptor.
pub enum CoreBinaryPatternDescriptor {
    UInt(u64),
    IntBits(u64),
    Bytes(u64),
    Bits(u64),
    Utf8,
    Utf16,
    Utf32,
    Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Data describing core binary pattern field.
pub struct CoreBinaryPatternField {
    pub name: String,
    pub descriptor: CoreBinaryPatternDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// CoreIR string-pattern segment.
///
/// Inputs:
/// - Canonical syntax-output string pattern text.
///
/// Outputs:
/// - Literal or capture segment for VM pattern planning.
///
/// Transformation:
/// - Preserves segment order without committing to a backend matcher.
pub enum CoreStringPatternSegment {
    Literal(String),
    Capture(CoreStringPatternCapture),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// CoreIR string-pattern capture.
///
/// Inputs:
/// - Capture slot shaped as `${name}` or `${name: Type}`.
///
/// Outputs:
/// - Binding name plus optional validated type annotation text.
///
/// Transformation:
/// - Keeps the capture target explicit for later CoreIR/VM conversion planning.
pub struct CoreStringPatternCapture {
    pub name: String,
    pub type_annotation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
            CorePattern::String(value) => format!("String({value})"),
            CorePattern::StringPattern(segments) => format!(
                "StringPattern({})",
                segments
                    .iter()
                    .map(CoreStringPatternSegment::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CorePattern::Atom(value) => format!("Atom({value})"),
            CorePattern::Tuple(elements) => format!(
                "Tuple({})",
                elements
                    .iter()
                    .map(CorePattern::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CorePattern::Alias { alias, pattern } => {
                format!("Alias({alias},{})", pattern.contract_text())
            }
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
            CorePattern::BinaryLayout { endian, fields } => format!(
                "BinaryLayout({};{})",
                endian.contract_text(),
                fields
                    .iter()
                    .map(CoreBinaryPatternField::contract_text)
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

impl CoreBinaryPatternEndian {
    fn contract_text(self) -> &'static str {
        match self {
            Self::Big => "big",
            Self::Little => "little",
        }
    }
}

impl CoreBinaryPatternField {
    fn contract_text(&self) -> String {
        format!("{}:{}", self.name, self.descriptor.contract_text())
    }
}

impl CoreBinaryPatternDescriptor {
    fn contract_text(self) -> String {
        match self {
            Self::UInt(width) => format!("UInt[{width}]"),
            Self::IntBits(width) => format!("IntBits[{width}]"),
            Self::Bytes(width) => format!("Bytes[{width}]"),
            Self::Bits(width) => format!("Bits[{width}]"),
            Self::Utf8 => "Utf8".to_string(),
            Self::Utf16 => "Utf16".to_string(),
            Self::Utf32 => "Utf32".to_string(),
            Self::Rest => "Rest".to_string(),
        }
    }
}

impl CoreStringPatternSegment {
    /// Renders a Core string-pattern segment as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: Core string-pattern segment.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts.
    ///
    /// Transformation:
    /// - Tags literal and capture segments explicitly so VM planning can
    ///   distinguish them from ordinary string literals.
    fn contract_text(&self) -> String {
        match self {
            CoreStringPatternSegment::Literal(value) => format!("Literal({value})"),
            CoreStringPatternSegment::Capture(capture) => capture.contract_text(),
        }
    }
}

impl CoreStringPatternCapture {
    /// Renders a Core string-pattern capture as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: Core capture payload.
    ///
    /// Output:
    /// - Stable capture name and optional annotation text.
    ///
    /// Transformation:
    /// - Serializes type annotations only when the source capture supplied one.
    fn contract_text(&self) -> String {
        match &self.type_annotation {
            Some(annotation) => format!("Capture({}:{annotation})", self.name),
            None => format!("Capture({})", self.name),
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
        format!("{}:{}", self.key, self.value.contract_text())
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
