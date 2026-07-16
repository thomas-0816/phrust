<?php

// Native tier gap (b), last brick: native->userland tail call. A scalar-int leaf
// whose terminator returns the result of a CallFunction to a NON-inlinable
// userland function is currently rejected (the callee has branches, so it cannot
// be inlined). The tail-call recognizer instead compiles the caller's argument
// prefix natively, returns a "tail-call requested" status to the VM bridge, and
// the bridge performs the userland call through the normal interpreter path — so
// behavior matches the interpreter by construction, with zero native re-entry.
//
// `classify` has `if` branches, so it is NOT inlinable and exercises this path
// (not the native->native inline pass). `sign_of_sum` is the recognized leaf: it
// computes `$a + $b` natively, then tail-calls `classify`.
//
// Native differential fixture; the native runtime gate runs this with the
// native tier off and on and asserts identical output, plus a diff against PHP
// 8.5.7.

function classify(int $n): int {
    if ($n < 0) {
        return -1;
    }
    if ($n === 0) {
        return 0;
    }
    return 1;
}

function sign_of_sum(int $a, int $b): int {
    return classify($a + $b);
}

for ($i = -3; $i <= 3; $i++) {
    echo sign_of_sum($i, 1), "\n";
}
