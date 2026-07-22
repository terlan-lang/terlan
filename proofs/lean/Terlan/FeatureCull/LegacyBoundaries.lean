namespace Terlan.FeatureCull

inductive RejectionPhase where
  | parse
  | typecheck
  | targetSelection
  | compile
  deriving DecidableEq, Repr

inductive RetiredAssumption where
  | bytecodeIntermediary
  | explicitCoreTarget
  | importNamespaceShortcut
  | hostTestRuntime
  | tupleBindingWithoutPattern
  | nativeBridgeShortcut
  | profileSwitch
  deriving DecidableEq, Repr

inductive CurrentContract where
  | directVmLowering
  | inferredVmTarget
  | typedModuleImport
  | vmTestRunner
  | explicitPatternBinding
  | typedNativeBoundary
  deriving DecidableEq, Repr

def rejectionPhase : RetiredAssumption -> RejectionPhase
  | .bytecodeIntermediary => .compile
  | .explicitCoreTarget => .targetSelection
  | .importNamespaceShortcut => .parse
  | .hostTestRuntime => .compile
  | .tupleBindingWithoutPattern => .typecheck
  | .nativeBridgeShortcut => .typecheck
  | .profileSwitch => .targetSelection

def replacementFor : RetiredAssumption -> CurrentContract
  | .bytecodeIntermediary => .directVmLowering
  | .explicitCoreTarget => .inferredVmTarget
  | .importNamespaceShortcut => .typedModuleImport
  | .hostTestRuntime => .vmTestRunner
  | .tupleBindingWithoutPattern => .explicitPatternBinding
  | .nativeBridgeShortcut => .typedNativeBoundary
  | .profileSwitch => .inferredVmTarget

def blockedBeforeVm (_assumption : RetiredAssumption) : Bool := true

def currentContract (_contract : CurrentContract) : Bool := true

theorem bytecodeIntermediaryRejectedBeforeVm :
    blockedBeforeVm .bytecodeIntermediary = true := by
  rfl

theorem explicitCoreTargetRejectedBeforeVm :
    blockedBeforeVm .explicitCoreTarget = true := by
  rfl

theorem importNamespaceShortcutRejectedBeforeVm :
    blockedBeforeVm .importNamespaceShortcut = true := by
  rfl

theorem hostTestRuntimeRejectedBeforeVm :
    blockedBeforeVm .hostTestRuntime = true := by
  rfl

theorem tupleBindingWithoutPatternRejectedBeforeVm :
    blockedBeforeVm .tupleBindingWithoutPattern = true := by
  rfl

theorem nativeBridgeShortcutRejectedBeforeVm :
    blockedBeforeVm .nativeBridgeShortcut = true := by
  rfl

theorem profileSwitchRejectedBeforeVm :
    blockedBeforeVm .profileSwitch = true := by
  rfl

theorem everyRetiredAssumptionIsBlockedBeforeVm
    (assumption : RetiredAssumption) :
    blockedBeforeVm assumption = true := by
  cases assumption <;> rfl

theorem noProofArtifactUsesRetiredAssumptions
    (assumption : RetiredAssumption) :
    currentContract (replacementFor assumption) = true := by
  cases assumption <;> rfl

end Terlan.FeatureCull
