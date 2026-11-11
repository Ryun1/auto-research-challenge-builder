{-# LANGUAGE BangPatterns #-}

-- | Judge-owned check that a value is fully evaluated, independent of any
-- NFData or NoThunks instance (the candidate owns some of those: #52).
--
-- Walks the heap from a value and counts what is not plain data:
--   * unevaluated closures (thunks, selectors, partial stack applications);
--   * function closures, which could hide deferred work behind a call.
-- Static closures (CAFs, top-level functions, constants) are program-wide,
-- not part of the value, and are skipped.
module HeapWalk (unevaluated) where

import GHC.Exts.Heap

-- | Closures inspected per value before giving up (cyclic or absurdly large).
budget :: Int
budget = 2000000

-- | (unevaluated closures, function closures) reachable from the value.
-- Nothing if the walk ran out of budget.
unevaluated :: a -> IO (Maybe (Int, Int))
unevaluated x = go budget 0 0 [asBox x]
  where
    go :: Int -> Int -> Int -> [Box] -> IO (Maybe (Int, Int))
    go _ !t !f [] = pure (Just (t, f))
    go 0 _ _ _ = pure Nothing
    go n !t !f (b : bs) = do
      c <- getBoxedClosureData b
      case classify c of
        Skip -> go (n - 1) t f bs
        Thunk -> go (n - 1) (t + 1) f bs
        Function -> go (n - 1) t (f + 1) bs
        Follow ps -> go (n - 1) t f (ps ++ bs)

data Kind = Skip | Thunk | Function | Follow [Box]

classify :: Closure -> Kind
classify c = case c of
  -- unboxed payloads and byte arrays hold no pointers
  IntClosure {} -> Skip
  WordClosure {} -> Skip
  Int64Closure {} -> Skip
  Word64Closure {} -> Skip
  AddrClosure {} -> Skip
  FloatClosure {} -> Skip
  DoubleClosure {} -> Skip
  ArrWordsClosure {} -> Skip
  ConstrClosure {info = i, ptrArgs = ps}
    | tipe i == CONSTR_NOCAF -> Skip
    | otherwise -> Follow ps
  MutArrClosure {mccPayload = ps} -> Follow ps
  SmallMutArrClosure {mccPayload = ps} -> Follow ps
  -- an evaluated thunk: follow it to its value
  IndClosure {info = i, indirectee = p}
    | tipe i == IND_STATIC -> Skip
    | otherwise -> Follow [p]
  BlackholeClosure {indirectee = p} -> Follow [p]
  FunClosure {info = i}
    | tipe i == FUN_STATIC -> Skip
    | otherwise -> Function
  PAPClosure {} -> Function
  BCOClosure {} -> Function
  ThunkClosure {info = i}
    | tipe i == THUNK_STATIC -> Skip
    | otherwise -> Thunk
  -- selectors, AP, AP_STACK, and anything unexpected in plain data (a thread
  -- still evaluating a black hole, mutable variables, ...): not evaluated.
  _ -> Thunk
