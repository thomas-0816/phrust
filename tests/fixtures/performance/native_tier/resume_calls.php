<?php

// Return-and-resume call composition: the native tier's non-tail userland
// calls. A leaf like `$a = h($x); return g($a);` (or nested `return g(h($x));`)
// compiles to a region that suspends at each call: it marshals the arguments
// into buffer slots and returns a call-request status; the VM performs the
// call through the normal interpreter path, writes the callee's int result
// into the site's slot, and re-enters the region at the site's resume offset
// over the same buffer. Every live value sits in the flat slot buffer, so
// re-entry needs no register state.
//
// Soundness shape: side exits exist only before the first call (a non-int
// leaf argument interprets the whole leaf); every callee is a same-unit plain
// userland function whose declared `: int` return proves the result slot Int
// (its own return coercion produced an int or threw — and a throw propagates
// with the leaf's frame on the stack instead of resuming). The leaf's final
// result still runs through its return-site coercion.
//
// Native differential fixture; the native runtime gate runs
// this with the native tier off and on and asserts identical output, and
// against the pinned PHP 8.5.7 reference when available.

function triple(int $x): int {
    return $x * 3;
}

function plus_ten(int $x): int {
    return $x + 10;
}

// Sequenced composition: two suspension points with a move between them.
function chain(int $x): int {
    $a = triple($x);
    return plus_ten($a);
}

// Nested composition: the inner call's result feeds the outer call directly.
function nested(int $x): int {
    return plus_ten(triple($x));
}

// Three-deep chain through locals.
function chain3(int $x): int {
    $a = triple($x);
    $b = plus_ten($a);
    return triple($b);
}

// Side-effect ordering: the callee echoes, so native-vs-interpreter divergence
// in call ordering or count would show directly in the output.
function loud(int $x): int {
    echo "call:", $x, "\n";
    return $x + 1;
}

function twice(int $x): int {
    // Compute prefix: the region only engages when enough arithmetic moves
    // native to pay for the resume loop (move-only glue stays interpreted).
    $seed = ($x * 7 + 3) % 1000;
    $mix = $seed ^ ($seed >> 2);
    $a = loud($mix);
    return loud($a);
}

// A throwing callee mid-chain: the first call's side effect (echo) has
// happened; the second call throws. The exception must propagate through the
// leaf's materialized frame with an interpreter-identical trace.
function boom(int $x): int {
    throw new RuntimeException("boom:" . $x);
}

function explode_late(int $x): int {
    $seed = ($x * 5 + 1) % 100;
    $bump = ($seed << 1) + 1;
    $a = loud($bump);
    return boom($a);
}

// A callee whose own return coercion produces the int (weak-mode numeric
// string): the resume slot still receives a genuine int.
function stringly(int $x): int {
    return "4" . "2"; // "42" -> int(42) via the callee's return coercion
}

function via_stringly(int $x): int {
    $spin = ($x * 3 + 2) % 50;
    $arg = ($spin ^ 1) + 0;
    $a = stringly($arg);
    return plus_ten($a);
}

// Basic compositions run natively after the first call suspends/resumes.
var_dump(chain(5));      // triple(5)=15, plus_ten -> int(25)
var_dump(nested(5));     // int(25)
var_dump(chain3(2));     // triple(2)=6, +10=16, *3 -> int(48)

// Hot loop over a resume leaf: the region is compiled once and re-driven.
$sum = 0;
for ($i = 0; $i < 200; $i++) {
    $sum = plus_ten($sum); // ordinary leaf feeding …
}
var_dump(chain($sum));    // $sum=2000 -> int(6010)

// Side-effect ordering (two calls, echo between suspensions), behind a
// compute prefix so the resume region engages: seed=(7*7+3)%1000=52,
// mix=52^13=57.
var_dump(twice(7));       // call:57, call:58 -> int(59)

// Non-int leaf argument: the entry guard side-exits before any call and the
// interpreter runs the whole leaf (its int parameter coerces "3" -> 3).
var_dump(chain("3"));     // int(19)

// Exception after a performed call: echo happened, then the throw propagates.
// seed=(1*5+1)%100=6, bump=13.
try {
    explode_late(1);
} catch (RuntimeException $e) {
    echo "caught:", $e->getMessage(), "\n"; // call:13 … caught:boom:14
}

// Callee-side return coercion feeding the chain: spin=(0*3+2)%50=2,
// arg=2^1+0=3, stringly -> 42, +10.
var_dump(via_stringly(0)); // int(52)
