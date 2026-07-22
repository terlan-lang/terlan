namespace Terlan.Core

inductive CoreType where
  | int
  deriving DecidableEq

inductive Expr where
  | int : Int -> Expr
  | add : Expr -> Expr -> Expr

def eval : Expr -> Int
  | .int value => value
  | .add left right => eval left + eval right

inductive HasType : Expr -> CoreType -> Prop where
  | int (value : Int) : HasType (.int value) .int
  | add {left right : Expr} :
      HasType left .int ->
      HasType right .int ->
      HasType (.add left right) .int

theorem addEvaluationPreservesInteger
    (left right : Int) :
    eval (.add (.int left) (.int right)) = left + right := by
  rfl

theorem typedIntegerExpressionEvaluates
    {expression : Expr}
    (_typed : HasType expression .int) :
    ∃ value, eval expression = value := by
  exact ⟨eval expression, rfl⟩

end Terlan.Core
