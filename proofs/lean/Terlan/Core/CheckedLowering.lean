namespace Terlan.Core.CheckedLowering

inductive SourceExpr where
  | int : Int -> SourceExpr
  | add : SourceExpr -> SourceExpr -> SourceExpr
  | spawn : SourceExpr -> SourceExpr
  deriving DecidableEq

inductive CoreExpr where
  | int : Int -> CoreExpr
  | add : CoreExpr -> CoreExpr -> CoreExpr
  | spawn : CoreExpr -> CoreExpr
  deriving DecidableEq

inductive CoreType where
  | int
  | process
  deriving DecidableEq

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

def sourceEval : SourceExpr -> Int
  | .int value => value
  | .add left right => sourceEval left + sourceEval right
  | .spawn body => sourceEval body

def coreEval : CoreExpr -> Int
  | .int value => value
  | .add left right => coreEval left + coreEval right
  | .spawn body => coreEval body

theorem loweringIsDeterministic
    (source : SourceExpr)
    (first second : CoreExpr)
    (firstResult : lower source = first)
    (secondResult : lower source = second) :
    first = second := by
  rw [← firstResult, ← secondResult]

theorem loweringPreservesTyping
    {source : SourceExpr}
    {coreType : CoreType}
    (typed : SourceHasType source coreType) :
    CoreHasType (lower source) coreType := by
  induction typed with
  | int value =>
      exact CoreHasType.int value
  | add _ _ leftTyped rightTyped =>
      exact CoreHasType.add leftTyped rightTyped
  | spawn _ bodyTyped =>
      exact CoreHasType.spawn bodyTyped

theorem loweringPreservesEvaluation
    (source : SourceExpr) :
    coreEval (lower source) = sourceEval source := by
  induction source with
  | int _ =>
      rfl
  | add _ _ leftResult rightResult =>
      simp [lower, coreEval, sourceEval, leftResult, rightResult]
  | spawn _ bodyResult =>
      simp [lower, coreEval, sourceEval, bodyResult]

inductive TargetProfile where
  | vm
  | jsShared
  | wasmCore
  deriving DecidableEq

def admits : TargetProfile -> SourceExpr -> Bool
  | .vm, _ => true
  | .jsShared, .spawn _ => false
  | .jsShared, _ => true
  | .wasmCore, .spawn _ => false
  | .wasmCore, _ => true

theorem vmAcceptsTypedProcessForm
    {body : SourceExpr}
    (_typed : SourceHasType body .int) :
    admits .vm (.spawn body) = true := by
  rfl

theorem jsRejectsProcessForm
    (body : SourceExpr) :
    admits .jsShared (.spawn body) = false := by
  rfl

theorem wasmRejectsProcessForm
    (body : SourceExpr) :
    admits .wasmCore (.spawn body) = false := by
  rfl

end Terlan.Core.CheckedLowering
