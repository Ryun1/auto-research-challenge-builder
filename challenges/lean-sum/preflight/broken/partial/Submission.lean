import Spec

/-- The sum 0 + 1 + ... + n by recursion. -/
def sumTo : Nat → Nat
  | 0 => 0
  | n + 1 => sumTo n + (n + 1)

theorem foldl_add_shift (l : List Nat) (a : Nat) :
    l.foldl (· + ·) a = a + l.foldl (· + ·) 0 := by
  induction l generalizing a with
  | nil => simp
  | cons x xs ih =>
    simp only [List.foldl]
    rw [ih (a + x), ih (0 + x)]
    omega

-- BROKEN: the correctness theorem is missing
