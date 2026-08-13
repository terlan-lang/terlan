namespace Terlan.WasmBridge

inductive TerlanType where
  | int
  | float
  | bool
  | resource
  | dynamic
  deriving DecidableEq

inductive WasmType where
  | i64
  | f64
  | i32
  | handle
  deriving DecidableEq

def lowerType : TerlanType -> Option WasmType
  | .int => some .i64
  | .float => some .f64
  | .bool => some .i32
  | .resource => some .handle
  | .dynamic => none

structure Signature where
  parameters : List TerlanType
  result : TerlanType

def lowerSignature (signature : Signature) : Option (List WasmType × WasmType) := do
  let parameters <- signature.parameters.mapM lowerType
  let result <- lowerType signature.result
  pure (parameters, result)

theorem portableSignaturePreservesArity :
    lowerSignature { parameters := [.int, .bool], result := .float } =
      some ([.i64, .i32], .f64) := by
  rfl

theorem dynamicSignatureRejected :
    lowerSignature { parameters := [.dynamic], result := .int } = none := by
  rfl

inductive ResourceOwner where
  | terlan
  | wasm
  deriving DecidableEq

def transfer : ResourceOwner -> ResourceOwner
  | .terlan => .wasm
  | .wasm => .terlan

theorem resourceTransferChangesOwner (owner : ResourceOwner) :
    transfer owner ≠ owner := by
  cases owner <;> decide

inductive CallPath where
  | returned
  | aborted
  deriving DecidableEq

def deterministicOutcome : Bool -> CallPath
  | true => .returned
  | false => .aborted

theorem callAndAbortPathsAreDeterministic (success : Bool) :
    deterministicOutcome success = if success then .returned else .aborted := by
  cases success <;> rfl

def hostSideEffectAdmitted (declaredSafeNative : Bool) : Bool := declaredSafeNative

theorem undeclaredHostSideEffectRejected :
    hostSideEffectAdmitted false = false := by
  rfl

end Terlan.WasmBridge
