<?php

// Scalar-int leaf functions covering the copy-and-patch native tier's subset:
// guarded arithmetic (add/sub/mul with overflow side exit), modulo and shifts
// with domain guards, bitwise ops, an int comparison returning bool, an
// if/else diamond, and a while loop. Each is a free function with typed int
// params and an int/bool return, so it is eligible for native execution; all
// are called from top-level (dense) code so the dense-path hook engages.
//
// Differential harness: scripts/performance/copy_patch_native_diff.py runs this
// with the native tier off and on and asserts identical output, and against the
// pinned PHP 8.5.7 reference when available.

function arith(int $a, int $b): int {
    return $a * $b + ($a - $b);
}

function modulo(int $a, int $b): int {
    return $a % $b;
}

function shifts(int $a, int $b): int {
    return ($a << $b) + ($a >> 1);
}

function bits(int $a, int $b): int {
    return ($a & $b) | ($a ^ $b);
}

function is_less(int $a, int $b): bool {
    return $a < $b;
}

function max2(int $a, int $b): int {
    if ($a > $b) {
        return $a;
    }
    return $b;
}

function sum_below(int $n): int {
    $s = 0;
    $i = 0;
    while ($i < $n) {
        $s = $s + $i;
        $i = $i + 1;
    }
    return $s;
}

function fma(float $a, float $b): float {
    return $a * $b + $a / 2.0;
}

$acc = 0;
$facc = 0.0;
for ($i = 0; $i < 8; $i++) {
    $acc = $acc + arith($i, 3);
    $acc = $acc + modulo($i, 5);
    $acc = $acc + shifts($i, 2);
    $acc = $acc + bits($i, 6);
    $acc = $acc + (is_less($i, 4) ? 100 : 200);
    $acc = $acc + max2($i, 4);
    $acc = $acc + sum_below($i);
    $facc = $facc + fma($i + 0.5, 1.5);
}
echo $acc, "\n";
echo $facc, "\n";
