/-!
Executable theorems for the stable syntax-output/CoreIR boundary.

These theorems deliberately begin at an observed generated-parser
classification. They verify the contract consumed by lowering; they do not
claim that the generated Rust parser implementation is formally verified.
-/

-- canonical-ebnf-sha256: f15e5e3b8d546024312cb9e097495f268c179e272e341e26c04fe2811fd53405

namespace Terlan.ParserShape

def canonicalEbnfSha256 : String :=
  "f15e5e3b8d546024312cb9e097495f268c179e272e341e26c04fe2811fd53405"

def syntaxOutputSchema : String := "terlan.lalrpop-module-output.v1"

structure GrammarIdentity where
  schema : String
  ebnfSha256 : String
  deriving DecidableEq

def canonicalGrammarIdentity : GrammarIdentity where
  schema := syntaxOutputSchema
  ebnfSha256 := canonicalEbnfSha256

theorem canonicalGrammarIdentityIsStable :
    canonicalGrammarIdentity =
      { schema := syntaxOutputSchema, ebnfSha256 := canonicalEbnfSha256 } := by
  rfl

inductive SyntaxClassification where
  | module
  | declaration
  | expression
  | pattern
  | typeExpression
  | assignment
  | erlangBinarySegment
  | negativeStructuralImplication
  deriving DecidableEq

def acceptedClassification : SyntaxClassification → Bool
  | .module
  | .declaration
  | .expression
  | .pattern
  | .typeExpression => true
  | .assignment
  | .erlangBinarySegment
  | .negativeStructuralImplication => false

theorem canonicalModuleClassificationAccepted :
    acceptedClassification .module = true := by
  rfl

theorem canonicalPatternClassificationAccepted :
    acceptedClassification .pattern = true := by
  rfl

theorem plainAssignmentClassificationRejected :
    acceptedClassification .assignment = false := by
  rfl

theorem erlangBinaryClassificationRejected :
    acceptedClassification .erlangBinarySegment = false := by
  rfl

theorem negativeStructuralImplicationClassificationRejected :
    acceptedClassification .negativeStructuralImplication = false := by
  rfl

inductive Operator where
  | pipe
  | disjunction
  | conjunction
  | comparison
  | addition
  | multiplication
  deriving DecidableEq

def precedence : Operator → Nat
  | .pipe => 10
  | .disjunction => 20
  | .conjunction => 30
  | .comparison => 40
  | .addition => 50
  | .multiplication => 60

inductive Associativity where
  | left
  | nonAssociative
  deriving DecidableEq

def associativity : Operator → Associativity
  | .comparison => .nonAssociative
  | _ => .left

theorem multiplicationBindsAboveAddition :
    precedence .multiplication > precedence .addition := by
  decide

theorem additionBindsAboveComparison :
    precedence .addition > precedence .comparison := by
  decide

theorem conjunctionBindsAboveDisjunction :
    precedence .conjunction > precedence .disjunction := by
  decide

theorem pipeHasLowestCanonicalPrecedence :
    precedence .pipe < precedence .disjunction := by
  decide

theorem arithmeticOperatorsAssociateLeft :
    associativity .addition = .left ∧
      associativity .multiplication = .left := by
  exact ⟨rfl, rfl⟩

inductive ExprShape where
  | atom (name : String)
  | binary (operator : Operator) (left right : ExprShape)
  deriving DecidableEq

def foldLeftThree
    (operator : Operator)
    (first second third : ExprShape) : ExprShape :=
  .binary operator (.binary operator first second) third

theorem generatedFoldIsLeftAssociative
    (operator : Operator)
    (first second third : ExprShape) :
    foldLeftThree operator first second third =
      .binary operator (.binary operator first second) third := by
  rfl

inductive ShapeKind where
  | scalar
  | tuple
  | record
  | list
  | map
  | bitstring
  | implContract
  | capabilityDenyList
  deriving DecidableEq

structure ShapeNode where
  kind : ShapeKind
  arity : Nat
  children : List String
  deriving DecidableEq

def ShapeNode.WellFormed (node : ShapeNode) : Prop :=
  match node.kind with
  | .scalar => node.arity = 0 ∧ node.children = []
  | .tuple => node.arity = node.children.length ∧ node.arity ≥ 2
  | .record => node.arity = node.children.length ∧ node.children.Nodup
  | .list => node.arity = 1 ∧ node.children.length = 1
  | .map => node.arity = node.children.length ∧ node.children.Nodup
  | .bitstring => node.arity = node.children.length ∧ node.arity > 0
  | .implContract => node.arity = node.children.length ∧ node.arity > 0
  | .capabilityDenyList =>
      node.arity = node.children.length ∧ node.children.Nodup

structure SyntaxOutput where
  schema : String
  grammar : GrammarIdentity
  root : ShapeNode
  deriving DecidableEq

def SyntaxOutput.WellFormed (output : SyntaxOutput) : Prop :=
  output.schema = syntaxOutputSchema ∧
    output.grammar = canonicalGrammarIdentity ∧
    output.root.WellFormed

def output (root : ShapeNode) : SyntaxOutput where
  schema := syntaxOutputSchema
  grammar := canonicalGrammarIdentity
  root

def tupleOutput (first second : String) : SyntaxOutput :=
  output { kind := .tuple, arity := 2, children := [first, second] }

def recordOutput (field : String) : SyntaxOutput :=
  output { kind := .record, arity := 1, children := [field] }

def listOutput (element : String) : SyntaxOutput :=
  output { kind := .list, arity := 1, children := [element] }

def mapOutput (field : String) : SyntaxOutput :=
  output { kind := .map, arity := 1, children := [field] }

def bitstringOutput (segment : String) : SyntaxOutput :=
  output { kind := .bitstring, arity := 1, children := [segment] }

def implContractOutput (capability : String) : SyntaxOutput :=
  output { kind := .implContract, arity := 1, children := [capability] }

def capabilityDenyOutput (capability : String) : SyntaxOutput :=
  output {
    kind := .capabilityDenyList
    arity := 1
    children := [capability]
  }

theorem tupleOutputWellFormed (first second : String) :
    (tupleOutput first second).WellFormed := by
  simp [tupleOutput, output, SyntaxOutput.WellFormed, ShapeNode.WellFormed]

theorem recordOutputWellFormed (field : String) :
    (recordOutput field).WellFormed := by
  simp [recordOutput, output, SyntaxOutput.WellFormed, ShapeNode.WellFormed]

theorem listOutputWellFormed (element : String) :
    (listOutput element).WellFormed := by
  simp [listOutput, output, SyntaxOutput.WellFormed, ShapeNode.WellFormed]

theorem mapOutputWellFormed (field : String) :
    (mapOutput field).WellFormed := by
  simp [mapOutput, output, SyntaxOutput.WellFormed, ShapeNode.WellFormed]

theorem bitstringOutputWellFormed (segment : String) :
    (bitstringOutput segment).WellFormed := by
  simp [bitstringOutput, output, SyntaxOutput.WellFormed, ShapeNode.WellFormed]

theorem implContractOutputWellFormed (capability : String) :
    (implContractOutput capability).WellFormed := by
  simp [
    implContractOutput,
    output,
    SyntaxOutput.WellFormed,
    ShapeNode.WellFormed
  ]

theorem capabilityDenyOutputWellFormed (capability : String) :
    (capabilityDenyOutput capability).WellFormed := by
  simp [
    capabilityDenyOutput,
    output,
    SyntaxOutput.WellFormed,
    ShapeNode.WellFormed
  ]

structure CheckedCoreIR where
  shapeKind : ShapeKind
  shapeArity : Nat
  fields : List String
  deniedCapabilities : List String
  deriving DecidableEq

def lowerChecked
    (syntaxTree : SyntaxOutput)
    (_wellFormed : syntaxTree.WellFormed) : CheckedCoreIR where
  shapeKind := syntaxTree.root.kind
  shapeArity := syntaxTree.root.arity
  fields := syntaxTree.root.children
  deniedCapabilities :=
    if syntaxTree.root.kind = .capabilityDenyList
    then syntaxTree.root.children
    else []

theorem loweringPreservesShapeKind
    (syntaxTree : SyntaxOutput)
    (wellFormed : syntaxTree.WellFormed) :
    (lowerChecked syntaxTree wellFormed).shapeKind = syntaxTree.root.kind := by
  rfl

theorem loweringPreservesShapeArity
    (syntaxTree : SyntaxOutput)
    (wellFormed : syntaxTree.WellFormed) :
    (lowerChecked syntaxTree wellFormed).shapeArity = syntaxTree.root.arity := by
  rfl

theorem loweringPreservesShapeFields
    (syntaxTree : SyntaxOutput)
    (wellFormed : syntaxTree.WellFormed) :
    (lowerChecked syntaxTree wellFormed).fields = syntaxTree.root.children := by
  rfl

theorem loweringPreservesCapabilityDenyList
    (syntaxTree : SyntaxOutput)
    (wellFormed : syntaxTree.WellFormed)
    (isDenyList : syntaxTree.root.kind = .capabilityDenyList) :
    (lowerChecked syntaxTree wellFormed).deniedCapabilities =
      syntaxTree.root.children := by
  simp [lowerChecked, isDenyList]

theorem checkedCapabilityDenyListHasNoDuplicates
    (syntaxTree : SyntaxOutput)
    (wellFormed : syntaxTree.WellFormed)
    (isDenyList : syntaxTree.root.kind = .capabilityDenyList) :
    (lowerChecked syntaxTree wellFormed).deniedCapabilities.Nodup := by
  have rootWellFormed : syntaxTree.root.WellFormed := wellFormed.2.2
  rw [ShapeNode.WellFormed, isDenyList] at rootWellFormed
  rw [loweringPreservesCapabilityDenyList syntaxTree wellFormed isDenyList]
  exact rootWellFormed.2

structure BoundaryEvidence (classification : SyntaxClassification) where
  observedAccepted : acceptedClassification classification = true

theorem acceptedBoundaryCanEnterCheckedLowering
    (syntaxTree : SyntaxOutput)
    (wellFormed : syntaxTree.WellFormed)
    (_evidence : BoundaryEvidence .module) :
    ∃ core, core = lowerChecked syntaxTree wellFormed := by
  exact ⟨lowerChecked syntaxTree wellFormed, rfl⟩

end Terlan.ParserShape
