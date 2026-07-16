<?php

// Packed-array element reads in the native tier: a register-indexed `$a[$i]`
// inside the scalar-int CFG subset lowers to a guarded helper call
// (`php_jit_array_fetch_int_slow`, the same safe facade the Cranelift packed
// fetch uses): an `OpaqueArray` tag guard on the array slot, an `Int` guard on
// the index, then a `blr` into the read-only bounds/layout-checked fetch.
// `array`-typed parameters are admitted into the CFG subset for exactly this
// op — they are only readable through it, and `array` declared types never
// coerce at bind.
//
// Side exits (the interpreter reproduces the exact value/diagnostic): a
// negative or out-of-bounds index (PHP's undefined-key warning is interpreter
// territory — not exercised here because the engine currently reports
// warnings on the diagnostic channel while the reference prints them to
// stdout, a pre-existing formatting gap in the PHPT baseline), a non-packed
// array (string keys / mixed storage), a non-int element (floats/strings),
// and an array holding a reference cell. A `??`-style quiet read rejects at
// recognition (its suppressed warning is indistinguishable from the loud one
// at the helper boundary).
//
// Native differential fixture; the native runtime gate runs
// this with the native tier off and on and asserts identical output, and
// against the pinned PHP 8.5.7 reference when available.

// The win case: a packed-int sum loop — fetch + add run fully native per
// iteration through the general CFG lowering.
function sum_arr(array $a, int $n): int {
    $s = 0;
    for ($i = 0; $i < $n; $i++) {
        $s = $s + $a[$i];
    }
    return $s;
}

// Single-fetch leaf.
function pick(array $a, int $i): int {
    return $a[$i];
}

// Quiet (null-coalescing) read: rejected at recognition; the interpreter
// runs the whole function. Identical output either way.
function pick_or(array $a, int $i): int {
    return $a[$i] ?? -1;
}

$xs = [10, 20, 30, 40, 50];

// Native fetches: loop sum, first, last.
var_dump(sum_arr($xs, 5));   // int(150)
var_dump(pick($xs, 0));      // int(10)
var_dump(pick($xs, 4));      // int(50)

// Hot loop over the compiled leaf.
$total = 0;
for ($i = 0; $i < 1000; $i++) {
    $total = ($total + pick($xs, $i % 5)) % 1000003;
}
var_dump($total);            // int(30000)

// Float elements: not packed-int, the fetch side-exits and the interpreter
// sums floats; the integral result passes the leaf's int return coercion.
var_dump(sum_arr([1.0, 2.0, 3.0], 3)); // int(6)

// Mixed storage (a string key): not packed, side-exits; the existing int
// entries still sum correctly in the interpreter.
$mixed = [0 => 1, 1 => 2, "k" => 99];
var_dump(sum_arr($mixed, 2)); // int(3)

// An array holding a reference cell: the packed-int guard rejects it (reads
// could observe the cell), so the interpreter reads through the reference.
$ref = [7, 8, 9];
$alias = &$ref[1];
var_dump(pick($ref, 1));     // int(8)
$alias = 80;
var_dump(pick($ref, 1));     // int(80)

// Quiet read: in-bounds hits the value, out-of-bounds takes the default with
// no warning (this shape never compiles; it pins the recognizer rejection).
var_dump(pick_or($xs, 2));   // int(30)
var_dump(pick_or($xs, 9));   // int(-1)

// A non-array argument side-exits at the array tag guard before the helper
// runs; the interpreter throws the exact bind-time TypeError.
try {
    pick("nope", 0);
} catch (\TypeError $t) {
    echo "TypeError\n";      // TypeError
}
