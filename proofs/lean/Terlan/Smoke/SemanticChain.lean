import Init.Omega

namespace Terlan.Smoke.SemanticChain

inductive SourceExpr where
  | int : Int -> SourceExpr
  | add : SourceExpr -> SourceExpr -> SourceExpr
  | spawn : SourceExpr -> SourceExpr

inductive CoreExpr where
  | int : Int -> CoreExpr
  | add : CoreExpr -> CoreExpr -> CoreExpr
  | spawn : CoreExpr -> CoreExpr

inductive CoreType where
  | int
  | process

inductive SourceHasType : SourceExpr -> CoreType -> Prop where
  | int (value : Int) : SourceHasType (.int value) .int
  | add {left right : SourceExpr} :
      SourceHasType left .int ->
      SourceHasType right .int ->
      SourceHasType (.add left right) .int
  | spawn {body : SourceExpr} :
      SourceHasType body .int ->
      SourceHasType (.spawn body) .process

inductive CoreHasType : CoreExpr -> CoreType -> Prop where
  | int (value : Int) : CoreHasType (.int value) .int
  | add {left right : CoreExpr} :
      CoreHasType left .int ->
      CoreHasType right .int ->
      CoreHasType (.add left right) .int
  | spawn {body : CoreExpr} :
      CoreHasType body .int ->
      CoreHasType (.spawn body) .process

def lower : SourceExpr -> CoreExpr
  | .int value => .int value
  | .add left right => .add (lower left) (lower right)
  | .spawn body => .spawn (lower body)

def coreEval : CoreExpr -> Int
  | .int value => value
  | .add left right => coreEval left + coreEval right
  | .spawn body => coreEval body

def loweringPreservesTyping
    {source : SourceExpr}
    {coreType : CoreType}
    (typed : SourceHasType source coreType) :
    CoreHasType (lower source) coreType := by
  induction typed with
  | int value => exact CoreHasType.int value
  | add _ _ leftTyped rightTyped => exact CoreHasType.add leftTyped rightTyped
  | spawn _ bodyTyped => exact CoreHasType.spawn bodyTyped

inductive TargetProfile where
  | vm
  | jsShared
  | wasmCore

def admits : TargetProfile -> SourceExpr -> Bool
  | .vm, _ => true
  | .jsShared, .spawn _ => false
  | .jsShared, _ => true
  | .wasmCore, .spawn _ => false
  | .wasmCore, _ => true

structure Admission where
  capability : Bool
  scheduler : Bool
  arity : Bool
  arguments : Bool
  resourceOwner : Bool

def admitted (plan : Admission) : Bool :=
  plan.capability &&
    plan.scheduler &&
    plan.arity &&
    plan.arguments &&
    plan.resourceOwner

def ownerAuthorized (owner caller : Nat) : Bool :=
  owner == caller

def sourceProgram : SourceExpr :=
  .add (.int 20) (.int 22)

theorem parserToCoreTyping :
    CoreHasType (lower sourceProgram) .int := by
  apply loweringPreservesTyping
  exact SourceHasType.add (SourceHasType.int 20) (SourceHasType.int 22)

theorem coreToVmEvaluation :
    coreEval (lower sourceProgram) = 42 := by
  rfl

theorem vmOwnsProcessExecution :
    admits .vm (.spawn sourceProgram) = true := by
  rfl

theorem unsupportedTargetsRejectProcessExecution :
    admits .jsShared (.spawn sourceProgram) = false ∧
      admits .wasmCore (.spawn sourceProgram) = false := by
  exact ⟨rfl, rfl⟩

def admittedNativeDispatch : Admission where
  capability := true
  scheduler := true
  arity := true
  arguments := true
  resourceOwner := true

theorem nativeBoundaryDispatchAdmitted :
    admitted admittedNativeDispatch = true := by
  rfl

theorem nativeBoundaryRejectsForeignOwner :
    ownerAuthorized 7 8 = false := by
  rfl

theorem semanticChain :
    CoreHasType (lower sourceProgram) .int ∧
      coreEval (lower sourceProgram) = 42 ∧
      admits .vm (.spawn sourceProgram) = true ∧
      admitted admittedNativeDispatch = true := by
  exact
    ⟨parserToCoreTyping, coreToVmEvaluation, vmOwnsProcessExecution,
      nativeBoundaryDispatchAdmitted⟩

end Terlan.Smoke.SemanticChain
