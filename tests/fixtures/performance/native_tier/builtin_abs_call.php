<?php

// Builtin abs() lowered to a native helper call — the safe subset of native
// tier gap (b): inside a scalar-int leaf, abs($int) is emitted as a real `blr`
// into the pure phrust_jit_abs_i64 VM helper over the slot ABI (no VM re-entry,
// no context pointer). The call is compiled natively only after the VM confirms
// `abs` resolves to the real builtin (not a user-defined or namespaced shadow).
//
// abs(PHP_INT_MIN) overflows i64, so PHP promotes it to a float; the native
// path side-exits there and the interpreter produces the float. That value is
// shown via magnitude(), whose int|float return keeps it in the interpreter —
// the exact value the native path's side exit defers to.
//
// Native differential fixture; the native runtime gate executes this
// with the native tier off and on and asserts identical output, and against the
// pinned PHP 8.5.7 reference when available.

// Native scalar-int leaf: the abs() result feeds further guarded arithmetic.
function abs_plus_one(int $x): int {
    return abs($x) + 1;
}

// abs() of a computed difference, still a native int leaf (the argument is a
// register produced by the subtraction, not a bare parameter load).
function abs_diff(int $a, int $b): int {
    return abs($a - $b);
}

// abs() magnitude with a float-accepting return: PHP promotes abs(PHP_INT_MIN)
// to a float, so this surfaces that float. Its union return type keeps it in
// the interpreter, which is where the native path side-exits to for that input.
function magnitude(int $x): int|float {
    return abs($x);
}

$acc = 0;
for ($i = 1; $i <= 8; $i++) {
    $acc = $acc + abs_plus_one(-$i);
    $acc = $acc + abs_plus_one($i);
    $acc = $acc + abs_diff($i, 2 * $i);
}
echo $acc, "\n";
echo abs_plus_one(-5), " ", abs_plus_one(3), "\n";
echo magnitude(PHP_INT_MIN), "\n";
