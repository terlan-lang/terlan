namespace Terlan.Type.ShapeImplication

inductive Visibility where
  | exported
  | hidden
  deriving DecidableEq

inductive FieldType where
  | int
  | bool
  | string
  | nestedShape (structuralFingerprint : String)
  deriving DecidableEq

abbrev Field := String × FieldType × Visibility
abbrev ClosedShape := List Field

def fieldName (field : Field) : String :=
  field.1

def WellFormed (shape : ClosedShape) : Prop :=
  shape ≠ [] ∧ (shape.map fieldName).Nodup

def Entails (source required : ClosedShape) : Prop :=
  ∀ field, field ∈ required → field ∈ source

inductive Provenance where
  | concreteClosedType
  | explicitDeclaration
  | importedInterface
  deriving DecidableEq

structure Evidence (source required : ClosedShape) where
  provenance : Provenance
  sourceWellFormed : WellFormed source
  requiredWellFormed : WellFormed required
  entails : Entails source required

abbrev ScopeId := Nat

structure ScopedEvidence (source required : ClosedShape) where
  scope : ScopeId
  evidence : Evidence source required

def ScopedEvidence.availableIn
    {source required : ClosedShape}
    (owned : ScopedEvidence source required)
    (current : ScopeId) : Prop :=
  current = owned.scope

noncomputable def check
    (source required : ClosedShape)
    (provenance : Provenance) : Option (Evidence source required) := by
  classical
  exact if sourceWellFormed : WellFormed source then
    if requiredWellFormed : WellFormed required then
      if proof : Entails source required then
        some { provenance, sourceWellFormed, requiredWellFormed, entails := proof }
      else
        none
    else
      none
  else
    none

theorem acceptedEvidenceIsWellFormed
    {source required : ClosedShape}
    (evidence : Evidence source required) :
    WellFormed source ∧ WellFormed required := by
  exact ⟨evidence.sourceWellFormed, evidence.requiredWellFormed⟩

theorem requiredFieldProjectionIsSound
    {source required : ClosedShape}
    (evidence : Evidence source required)
    {field : Field}
    (requiredField : field ∈ required) :
    field ∈ source := by
  exact evidence.entails field requiredField

theorem provenImplicationIsAccepted
    {source required : ClosedShape}
    (sourceWellFormed : WellFormed source)
    (requiredWellFormed : WellFormed required)
    (proof : Entails source required)
    (provenance : Provenance) :
    ∃ evidence, check source required provenance = some evidence := by
  simp [check, sourceWellFormed, requiredWellFormed, proof]

theorem unprovenImplicationIsRejected
    {source required : ClosedShape}
    (unproven : ¬ Entails source required)
    (provenance : Provenance) :
    check source required provenance = none := by
  simp [check, unproven]

theorem privateFieldCannotProvePublic
    (_name : String)
    (fieldType : FieldType) :
    ¬ Entails
      [(_name, fieldType, .hidden)]
      [(_name, fieldType, .exported)] := by
  intro evidence
  have publicFieldPresent := evidence
    (_name, fieldType, .exported)
    (by simp)
  simp at publicFieldPresent

theorem evidenceProvenanceIsPreserved
    {source required : ClosedShape}
    (evidence : Evidence source required) :
    evidence.provenance = evidence.provenance := by
  rfl

theorem scopedEvidenceCannotEscape
    {source required : ClosedShape}
    (owned : ScopedEvidence source required)
    (current : ScopeId)
    (outside : current ≠ owned.scope) :
    ¬ owned.availableIn current := by
  exact outside

theorem scopedRequiredFieldProjectionIsSound
    {source required : ClosedShape}
    (owned : ScopedEvidence source required)
    (current : ScopeId)
    (_available : owned.availableIn current)
    {field : Field}
    (requiredField : field ∈ required) :
    field ∈ source := by
  exact owned.evidence.entails field requiredField

def applyEvidence
    {source required : ClosedShape}
    {Value : Type}
    (_evidence : Evidence source required)
    (value : Value) : Value :=
  value

theorem implicationEvidenceDoesNotConvert
    {source required : ClosedShape}
    {Value : Type}
    (evidence : Evidence source required)
    (value : Value) :
    applyEvidence evidence value = value := by
  rfl

def applyScopedEvidence
    {source required : ClosedShape}
    {Value : Type}
    (owned : ScopedEvidence source required)
    (current : ScopeId)
    (_available : owned.availableIn current)
    (value : Value) : Value :=
  value

theorem scopedEvidenceDoesNotConvert
    {source required : ClosedShape}
    {Value : Type}
    (owned : ScopedEvidence source required)
    (current : ScopeId)
    (available : owned.availableIn current)
    (value : Value) :
    applyScopedEvidence owned current available value = value := by
  rfl

def evaluateFunctionWithEvidence
    {source required : ClosedShape}
    {Value Result : Type}
    (owned : ScopedEvidence source required)
    (current : ScopeId)
    (available : owned.availableIn current)
    (function : Value → Result)
    (value : Value) : Result :=
  function (applyScopedEvidence owned current available value)

theorem implicationPreservesFunctionResult
    {source required : ClosedShape}
    {Value Result : Type}
    (owned : ScopedEvidence source required)
    (current : ScopeId)
    (available : owned.availableIn current)
    (function : Value → Result)
    (value : Value) :
    evaluateFunctionWithEvidence owned current available function value =
      function value := by
  rfl

def evaluateBranchWithEvidence
    {source required : ClosedShape}
    {Value Result : Type}
    (owned : ScopedEvidence source required)
    (current : ScopeId)
    (available : owned.availableIn current)
    (condition : Bool)
    (onRequired : Value → Result)
    (onFallback : Result)
    (value : Value) : Result :=
  if condition then
    onRequired (applyScopedEvidence owned current available value)
  else
    onFallback

theorem implicationPreservesBranchResult
    {source required : ClosedShape}
    {Value Result : Type}
    (owned : ScopedEvidence source required)
    (current : ScopeId)
    (available : owned.availableIn current)
    (condition : Bool)
    (onRequired : Value → Result)
    (onFallback : Result)
    (value : Value) :
    evaluateBranchWithEvidence
        owned current available condition onRequired onFallback value =
      if condition then onRequired value else onFallback := by
  cases condition <;> rfl

end Terlan.Type.ShapeImplication
