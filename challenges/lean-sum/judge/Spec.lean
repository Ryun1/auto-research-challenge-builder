/-! The judge's statement, elaborated before the candidate is seen, so no
    candidate instance or notation can change what `spec` means. -/
namespace Spec

/-- 0 + 1 + ... + n, the slow way. -/
def spec (n : Nat) : Nat := (List.range (n + 1)).foldl (· + ·) 0

end Spec
