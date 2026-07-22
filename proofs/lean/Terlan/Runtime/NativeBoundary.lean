import Init.Omega

namespace Terlan.Runtime.NativeBoundary

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

theorem admissionRequiresCapability
    {plan : Admission}
    (accepted : admitted plan = true) :
  plan.capability = true := by
  simp [admitted] at accepted
  exact accepted.1.1.1.1

theorem admissionRequiresScheduler
    {plan : Admission}
    (accepted : admitted plan = true) :
  plan.scheduler = true := by
  simp [admitted] at accepted
  exact accepted.1.1.1.2

theorem admissionRequiresValidatedArguments
    {plan : Admission}
    (accepted : admitted plan = true) :
  plan.arity = true ∧ plan.arguments = true := by
  simp [admitted] at accepted
  exact ⟨accepted.1.1.2, accepted.1.2⟩

theorem admissionRequiresResourceOwnership
    {plan : Admission}
    (accepted : admitted plan = true) :
  plan.resourceOwner = true := by
  simp [admitted] at accepted
  exact accepted.2

abbrev ProcessId := Nat

def ownerAuthorized (owner caller : ProcessId) : Bool :=
  owner == caller

theorem foreignOwnerRejected
    {owner caller : ProcessId}
    (different : owner ≠ caller) :
    ownerAuthorized owner caller = false := by
  simp [ownerAuthorized, different]

theorem ownerAccepted (owner : ProcessId) :
    ownerAuthorized owner owner = true := by
  simp [ownerAuthorized]

def creditsConserved (limit reserved available : Nat) : Prop :=
  reserved + available = limit

theorem completionPreservesCredits
    {limit reserved available : Nat}
    (reservedPositive : 0 < reserved)
    (conserved : creditsConserved limit reserved available) :
    creditsConserved limit (reserved - 1) (available + 1) := by
  simp only [creditsConserved] at conserved ⊢
  omega

theorem cancellationPreservesCredits
    {limit reserved available : Nat}
    (reservedPositive : 0 < reserved)
    (conserved : creditsConserved limit reserved available) :
    creditsConserved limit (reserved - 1) (available + 1) := by
  exact completionPreservesCredits reservedPositive conserved

end Terlan.Runtime.NativeBoundary
