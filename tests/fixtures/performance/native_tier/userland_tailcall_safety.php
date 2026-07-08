<?php

// SAFETY fixture for the native->userland tail-call recognizer. Every scenario
// must produce byte-identical output with the native tier off and on: when the
// native prefix's Int guard fails, or the VM's call-time validation rejects the
// callee, execution falls back to the interpreter and behaves identically.
//
// Differential: scripts/performance/copy_patch_native_diff.py runs this native
// off vs on and against PHP 8.5.7.

// A recognized tail-call leaf: computes an int arg natively, then tail-calls a
// non-inlinable userland function (has branches).
function step(int $n): int {
    if ($n < 0) {
        return 0;
    }
    return $n * 2;
}

function relay(int $x): int {
    return step($x + 1);
}

// Callee takes a by-reference parameter: the VM's call-time validation must
// reject the tail call (out of scope) and run the interpreter instead.
function bump(int &$slot): int {
    $slot = $slot + 100;
    return $slot;
}

function via_by_ref(int $x): int {
    return bump($x);
}

// Callee is a builtin (intdiv): also out of scope; the VM falls back.
function via_builtin(int $x): int {
    return intdiv($x, 10);
}

// Int args: the native tail-call path engages for relay.
echo relay(3), "\n";    // step(4)  = 8
echo relay(-5), "\n";   // step(-4) = 0

// A non-int actual arg (whole float, lossless coercion) trips the native Int
// guard and side-exits to the interpreter, which coerces 4.0 -> 4.
echo relay(4.0), "\n";  // step(5)  = 10

// A by-reference callee: the VM rejects the tail call and interprets it.
echo via_by_ref(7), "\n";   // bump: 7 + 100 = 107

// A builtin callee: out of scope, interpreter runs it.
echo via_builtin(95), "\n"; // intdiv(95, 10) = 9
