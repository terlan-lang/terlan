namespace Terlan.DatabaseSql

inductive SqlType where
  | int
  | text
  | bool
  deriving DecidableEq

structure Statement where
  parameters : List SqlType
  result : List SqlType

def bindingShapeAccepted (statement : Statement) (arguments : List SqlType) : Bool :=
  statement.parameters == arguments

theorem matchingPreparedStatementAccepted :
    bindingShapeAccepted
      { parameters := [.int, .text], result := [.bool] }
      [.int, .text] = true := by
  rfl

theorem wrongParameterArityRejected :
    bindingShapeAccepted
      { parameters := [.int, .text], result := [.bool] }
      [.int] = false := by
  rfl

theorem wrongParameterTypeRejected :
    bindingShapeAccepted
      { parameters := [.int], result := [.bool] }
      [.text] = false := by
  rfl

inductive TransactionState where
  | idle
  | active
  | committed
  | rolledBack
  deriving DecidableEq

def begin : TransactionState -> Option TransactionState
  | .idle => some .active
  | _ => none

def commit : TransactionState -> Option TransactionState
  | .active => some .committed
  | _ => none

def rollback : TransactionState -> Option TransactionState
  | .active => some .rolledBack
  | _ => none

theorem beginThenCommitIsWellFormed :
    (begin .idle).bind commit = some .committed := by
  rfl

theorem commitWithoutBeginRejected : commit .idle = none := by
  rfl

theorem rollbackReturnsTerminalState : rollback .active = some .rolledBack := by
  rfl

inductive EffectRegion where
  | pure
  | database
  deriving DecidableEq

def databaseOperationAccepted : EffectRegion -> Bool
  | .pure => false
  | .database => true

theorem databaseEffectRejectedInPureRegion :
    databaseOperationAccepted .pure = false := by
  rfl

theorem databaseEffectAcceptedInDatabaseRegion :
    databaseOperationAccepted .database = true := by
  rfl

end Terlan.DatabaseSql
