<?php

// Native tier gap (b), first brick: native->native inlining. A scalar-int/float
// leaf that calls another same-unit scalar leaf is currently rejected (contains
// a Call). With single-level inlining of a recognized leaf callee, the whole
// function compiles natively. These callers are pure scalar arithmetic delegating
// to small leaves — the shape inlining unlocks.
//
// Differential: scripts/performance/copy_patch_native_diff.py runs this with the
// native tier off and on and asserts identical output, plus a diff against PHP
// 8.5.7. Every value here is int/float so the whole thing is native-eligible once
// the callees inline.

function fma(int $a, int $b): int {
    return $a * $b + $a;
}

function scale(int $x): int {
    // Two calls to a same-unit scalar leaf, result combined with arithmetic.
    return fma($x, 3) + fma($x, 5);
}

function poly(int $x): int {
    // Nested delegation: poly -> scale -> fma.
    return scale($x) - $x;
}

function faverage(float $a, float $b): float {
    return ($a + $b) / 2.0;
}

function fblend(float $x): float {
    return faverage($x, 1.0) + faverage($x, 3.0);
}

$acc = 0;
$facc = 0.0;
for ($i = 0; $i < 12; $i++) {
    $acc = $acc + poly($i);
    $facc = $facc + fblend($i + 0.25);
}
echo $acc, "\n";
echo $facc, "\n";
