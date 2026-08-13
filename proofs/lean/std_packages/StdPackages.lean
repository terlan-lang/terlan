namespace Terlan.StdPackages

inductive JsonValue where
  | null
  | bool : Bool -> JsonValue
  | int : Int -> JsonValue
  | text : String -> JsonValue
  deriving DecidableEq

inductive JsonToken where
  | null
  | bool : Bool -> JsonToken
  | int : Int -> JsonToken
  | text : String -> JsonToken
  | invalid
  deriving DecidableEq

def encode : JsonValue -> JsonToken
  | .null => .null
  | .bool value => .bool value
  | .int value => .int value
  | .text value => .text value

def decode : JsonToken -> Option JsonValue
  | .null => some .null
  | .bool value => some (.bool value)
  | .int value => some (.int value)
  | .text value => some (.text value)
  | .invalid => none

theorem jsonRoundTrip (value : JsonValue) : decode (encode value) = some value := by
  cases value <;> rfl

theorem invalidJsonIsTypedFailure : decode .invalid = none := by
  rfl

def binaryStringRoundTrip (bytes : List UInt8) : List UInt8 := bytes

theorem binaryStringConversionPreservesBytes (bytes : List UInt8) :
    binaryStringRoundTrip bytes = bytes := by
  rfl

inductive TypedResult (α : Type) where
  | ok : α -> TypedResult α
  | error : String -> TypedResult α

def requireSome {α : Type} : Option α -> TypedResult α
  | some value => .ok value
  | none => .error "missing"

theorem missingOptionReturnsTypedError :
    requireSome (α := Nat) none = .error "missing" := by
  rfl

structure TimerEvent where
  scheduled : Nat
  fired : Nat

def timerValid (event : TimerEvent) : Bool := event.scheduled ≤ event.fired

theorem monotonicTimerAccepted : timerValid { scheduled := 4, fired := 7 } = true := by
  rfl

theorem earlyTimerRejected : timerValid { scheduled := 7, fired := 4 } = false := by
  rfl

end Terlan.StdPackages
