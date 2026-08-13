namespace Terlan.Concurrency

structure Message where
  sender : Nat
  priority : Nat
  sequence : Nat
  deriving DecidableEq

def send (mailbox : List Message) (message : Message) : List Message :=
  mailbox ++ [message]

theorem sendAppendsExactlyOne (mailbox : List Message) (message : Message) :
    (send mailbox message).length = mailbox.length + 1 := by
  simp [send]

theorem sameSenderOrderIsPreserved
    (mailbox : List Message) (first second : Message) :
    send (send mailbox first) second = mailbox ++ [first, second] := by
  simp [send, List.append_assoc]

def schedulerStep : List Nat -> List Nat
  | [] => []
  | _ :: remaining => remaining

theorem runnableQueueMakesBoundedProgress (actor : Nat) (remaining : List Nat) :
    (schedulerStep (actor :: remaining)).length < (actor :: remaining).length := by
  simp [schedulerStep]

def selectiveReceive (wanted : Nat) (mailbox : List Message) : Option Message :=
  mailbox.find? (fun message => message.sender == wanted)

theorem selectiveReceiveEmptyTerminates (wanted : Nat) :
    selectiveReceive wanted [] = none := by
  rfl

def timeoutOutcome (ticks : Nat) : Bool := ticks == 0

theorem zeroTimeoutTerminates : timeoutOutcome 0 = true := by
  rfl

def implicitOtpSchedulingGuarantee : Bool := false

theorem unsupportedImplicitOtpSchedulingRejected :
    implicitOtpSchedulingGuarantee = false := by
  rfl

end Terlan.Concurrency
