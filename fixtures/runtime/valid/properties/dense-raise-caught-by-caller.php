<?php
// Regression: a catchable Error raised inside a dense-planned body and caught
// by the caller must not corrupt the frame stack. propagate_exception pops
// the raising frame; the dense dispatch arms previously popped again,
// recycling the caller's frame (internal panic on the next register access).
class K { public static int $n = 0; }
function f() { return K::$missing; }
try { f(); } catch (Error $e) { echo "caught: ", $e->getMessage(), "\n"; }
echo "after\n";
