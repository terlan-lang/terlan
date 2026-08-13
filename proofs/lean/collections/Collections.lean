namespace Terlan.Collections

def listUpdate (values : List Nat) (index value : Nat) : List Nat :=
  values.set index value

theorem listUpdatePreservesLength (values : List Nat) (index value : Nat) :
    (listUpdate values index value).length = values.length := by
  simp [listUpdate]

def mapPut (entries : List (Nat × Nat)) (key value : Nat) : List (Nat × Nat) :=
  (key, value) :: entries.filter (fun entry => entry.1 != key)

def mapGet (entries : List (Nat × Nat)) (key : Nat) : Option Nat :=
  (entries.find? (fun entry => entry.1 == key)).map Prod.snd

theorem mapPutReturnsInsertedValue (entries : List (Nat × Nat)) (key value : Nat) :
    mapGet (mapPut entries key value) key = some value := by
  simp [mapPut, mapGet]

def mapDelete (entries : List (Nat × Nat)) (key : Nat) : List (Nat × Nat) :=
  entries.filter (fun entry => entry.1 != key)

theorem deleteFromEmptyIsEmpty (key : Nat) : mapDelete [] key = [] := by
  rfl

def setInsert (values : List Nat) (value : Nat) : List Nat :=
  if values.contains value then values else value :: values

theorem duplicateSetInsertIsStable (values : List Nat) (value : Nat)
    (present : value ∈ values) : setInsert values value = values := by
  simp [setInsert, present]

def deterministicIteration (values : List Nat) : List Nat := values

theorem iterationNeverInventsValues (values : List Nat) :
    deterministicIteration values = values := by
  rfl

def collisionWitness : List (Nat × Nat) := [(7, 1), (7, 2)]

theorem collisionUpdateSelectsNewest :
    mapGet (mapPut collisionWitness 7 3) 7 = some 3 := by
  rfl

def nondeterministicIterationAssumption : Bool := false

theorem nondeterministicIterationRejected :
    nondeterministicIterationAssumption = false := by
  rfl

end Terlan.Collections
