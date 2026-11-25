Multi-asset largest-first that prices the min-ada of token change as if it carried
no tokens: the token change gets the ADA-only minimum and the rest of the leftover
goes to a second, ADA-only change. The token change is below its real min-ada.
Preflight expects this to build and then be REJECTED by the judge (change below min_ada).
