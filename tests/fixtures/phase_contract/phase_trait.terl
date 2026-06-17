module phase_trait.

pub trait Eq[A] {
    equal(Left: A, Right: A): Bool.
}.

pub is_same[A](Left: A, Right: A)[Eq[A]]: Bool ->
    Eq.equal(Left, Right).
