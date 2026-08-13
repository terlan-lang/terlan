namespace Terlan.NativeBoundary

abbrev ProcessId := Nat

structure ResourceHandle where
  owner : ProcessId
  slot : Nat
  generation : Nat
deriving DecidableEq

def sameResource (left right : ResourceHandle) : Bool :=
  left.owner == right.owner &&
    left.slot == right.slot &&
    left.generation == right.generation

theorem distinctGenerationDoesNotAlias
    (handle : ResourceHandle)
    {nextGeneration : Nat}
    (different : nextGeneration ≠ handle.generation) :
    sameResource handle { handle with generation := nextGeneration } = false := by
  have reverse : handle.generation ≠ nextGeneration := Ne.symm different
  simp [sameResource, reverse]

def ownerAuthorized (caller : ProcessId) (handle : ResourceHandle) : Bool :=
  caller == handle.owner

theorem foreignOwnerRejected
    {caller : ProcessId}
    {handle : ResourceHandle}
    (foreign : caller != handle.owner) :
    ownerAuthorized caller handle = false := by
  simpa [ownerAuthorized] using foreign

theorem owningProcessAccepted (handle : ResourceHandle) :
    ownerAuthorized handle.owner handle = true := by
  simp [ownerAuthorized]

inductive HandleState where
  | live
  | consumed
deriving DecidableEq

def consumeHandle : HandleState → Option HandleState
  | .live => some .consumed
  | .consumed => none

theorem liveHandleIsConsumedExactlyOnce :
    consumeHandle .live = some .consumed := by
  rfl

theorem consumedHandleCannotBeConsumedAgain :
    consumeHandle .consumed = none := by
  rfl

structure ExportContract where
  arity : Nat
  argumentTypes : List String
  returnType : String

structure Callsite where
  argumentTypes : List String
  expectedReturnType : String

def callsiteConsistent (contract : ExportContract) (callsite : Callsite) : Prop :=
  contract.arity = contract.argumentTypes.length ∧
    callsite.argumentTypes = contract.argumentTypes ∧
    callsite.expectedReturnType = contract.returnType

theorem admittedCallPreservesArity
    {contract : ExportContract}
    {callsite : Callsite}
    (consistent : callsiteConsistent contract callsite) :
    callsite.argumentTypes.length = contract.arity := by
  rw [consistent.2.1, consistent.1]

theorem admittedCallPreservesArgumentTypes
    {contract : ExportContract}
    {callsite : Callsite}
    (consistent : callsiteConsistent contract callsite) :
    callsite.argumentTypes = contract.argumentTypes := by
  exact consistent.2.1

theorem admittedCallPreservesReturnType
    {contract : ExportContract}
    {callsite : Callsite}
    (consistent : callsiteConsistent contract callsite) :
    callsite.expectedReturnType = contract.returnType := by
  exact consistent.2.2

inductive HostEffect where
  | pureValue
  | file
  | socket
  | timer
  | processSpawn
  | processRegistry
  | acmeTls
deriving DecidableEq

def requiresAsyncCapabilityRpc : HostEffect → Bool
  | .pureValue => false
  | .file
  | .socket
  | .timer
  | .processSpawn
  | .processRegistry
  | .acmeTls => true

structure AsyncPolicy where
  effect : HostEffect
  capabilityRpc : Bool
  blocksShardOwner : Bool

def asyncPolicyCompliant (policy : AsyncPolicy) : Prop :=
  (requiresAsyncCapabilityRpc policy.effect = true → policy.capabilityRpc = true) ∧
    policy.blocksShardOwner = false

theorem sideEffectRequiresAsyncCapabilityRpc
    {policy : AsyncPolicy}
    (sideEffect : requiresAsyncCapabilityRpc policy.effect = true)
    (compliant : asyncPolicyCompliant policy) :
    policy.capabilityRpc = true := by
  exact compliant.1 sideEffect

theorem compliantExportNeverBlocksShardOwner
    {policy : AsyncPolicy}
    (compliant : asyncPolicyCompliant policy) :
    policy.blocksShardOwner = false := by
  exact compliant.2

inductive Capability where
  | file
  | socket
  | timer
  | processSpawn
  | processRegistry
  | acmeTls
deriving DecidableEq

def sideEffectDenyList : List Capability :=
  [
    .file,
    .socket,
    .timer,
    .processSpawn,
    .processRegistry,
    .acmeTls
  ]

def denied (capability : Capability) : Prop :=
  capability ∈ sideEffectDenyList

theorem fileEffectIsDenyListed : denied .file := by
  simp [denied, sideEffectDenyList]

theorem socketEffectIsDenyListed : denied .socket := by
  simp [denied, sideEffectDenyList]

theorem timerEffectIsDenyListed : denied .timer := by
  simp [denied, sideEffectDenyList]

theorem processSpawnEffectIsDenyListed : denied .processSpawn := by
  simp [denied, sideEffectDenyList]

theorem processRegistryEffectIsDenyListed : denied .processRegistry := by
  simp [denied, sideEffectDenyList]

theorem acmeTlsEffectIsDenyListed : denied .acmeTls := by
  simp [denied, sideEffectDenyList]

structure CapabilityPolicy where
  admitted : Capability → Bool

def denyListSound (policy : CapabilityPolicy) : Prop :=
  ∀ capability, denied capability → policy.admitted capability = false

theorem deniedSideEffectCannotBeAdmitted
    {policy : CapabilityPolicy}
    (sound : denyListSound policy)
    {capability : Capability}
    (isDenied : denied capability) :
    policy.admitted capability = false := by
  exact sound capability isDenied

inductive RuntimeAssumption where
  | terlanVm
  | beamNif
deriving DecidableEq

inductive HandleEncoding where
  | typed
  | untyped
deriving DecidableEq

structure BoundaryUsage where
  runtime : RuntimeAssumption
  handleEncoding : HandleEncoding

def safeBoundaryUsage (usage : BoundaryUsage) : Prop :=
  usage.runtime = .terlanVm ∧ usage.handleEncoding = .typed

theorem beamNifAssumptionRejected
    (encoding : HandleEncoding) :
    ¬ safeBoundaryUsage ⟨.beamNif, encoding⟩ := by
  simp [safeBoundaryUsage]

theorem untypedHandleRejected
    (runtime : RuntimeAssumption) :
    ¬ safeBoundaryUsage ⟨runtime, .untyped⟩ := by
  simp [safeBoundaryUsage]

structure ManifestRow where
  moduleName : String
  functionName : String
  operation : String
  arity : Nat
  argumentTypes : List String
  rowDigest : String

def manifestRowWellFormed (row : ManifestRow) : Prop :=
  row.moduleName ≠ "" ∧
    row.functionName ≠ "" ∧
    row.operation ≠ "" ∧
    row.arity = row.argumentTypes.length ∧
    row.rowDigest ≠ ""

def theoremManifestBinding
    (proofDigest : String)
    (row : ManifestRow) : Prop :=
  proofDigest ≠ "" ∧ manifestRowWellFormed row

theorem boundManifestRowHasTypedArity
    {proofDigest : String}
    {row : ManifestRow}
    (bound : theoremManifestBinding proofDigest row) :
    row.arity = row.argumentTypes.length := by
  exact bound.2.2.2.2.1

theorem boundManifestRowHasContentAddresses
    {proofDigest : String}
    {row : ManifestRow}
    (bound : theoremManifestBinding proofDigest row) :
    proofDigest ≠ "" ∧ row.rowDigest ≠ "" := by
  exact ⟨bound.1, bound.2.2.2.2.2⟩

theorem typedVmDispatchAdmitted
    {contract : ExportContract}
    {callsite : Callsite}
    {policy : AsyncPolicy}
    (typed : callsiteConsistent contract callsite)
    (asyncSafe : asyncPolicyCompliant policy) :
    callsite.argumentTypes.length = contract.arity ∧
      policy.blocksShardOwner = false := by
  exact ⟨admittedCallPreservesArity typed, asyncSafe.2⟩

end Terlan.NativeBoundary
