<?php

// Builtin count() lowered to a native helper call — the first heap-handle shape
// of the Cranelift native compiler. Inside a scalar leaf, count($array) is
// emitted as a real `blr` into the runtime `php_jit_array_len` helper over the
// slot ABI: the array crosses as a read-only borrowed handle (an OpaqueArray
// slot whose payload is a `*const Value` valid only for the synchronous call).
//
// Only the plain packed all-int array case runs natively. Every other shape —
// an associative/hashed array, a non-int-element array, a Countable object, or
// the PHP 8 TypeError of a non-countable scalar — trips a guard (the array-tag
// guard, or the helper's packed-int layout guard) and side-exits, so the
// interpreter reproduces the exact semantics and errors. A user-defined
// (namespaced) count shadow is never lowered: its call name carries the
// namespace, so the recognizer never matches the bare builtin.
//
// The native path is only taken after the VM confirms `count` resolves to the
// real builtin (see NativeCallPermits::builtin_count), mirroring abs().
//
// Native differential fixture; the native runtime gate executes this
// with the native tier off and on and asserts identical output, and against the
// pinned PHP 8.5.7 reference when available.

// A namespaced shadow of count(): calls inside the namespace resolve to this
// function, and its lowered call name carries the namespace, so the native path
// never matches it — the interpreter runs it, returning the shadow's value.
namespace Shadowed {
    function count($ignored): int {
        return 999;
    }

    function shadowed_count($a): int {
        return count($a);
    }
}

namespace {
    // The native count leaf: count($array) on a by-value parameter.
    function counter($a): int {
        return count($a);
    }

    // Packed all-int array: the native fast path runs the helper and returns the
    // length directly.
    echo counter([10, 20, 30, 40]), "\n";           // 4
    // Empty array.
    echo counter([]), "\n";                          // 0
    // Associative (hashed) array: not a packed layout, so the helper reports a
    // non-OK status and the stencil side-exits to the interpreter.
    echo counter(['x' => 1, 'y' => 2, 'z' => 3]), "\n"; // 3
    // Packed keys but non-int elements: not a packed-int layout -> side-exit.
    echo counter([1, 'two', 3.5]), "\n";             // 3

    // Countable object: count() dispatches to ->count(); the array-tag guard
    // fails (an object marshals as Uninitialized), so the interpreter runs it.
    class Bag implements \Countable {
        public function count(): int {
            return 7;
        }
    }
    echo counter(new Bag()), "\n";                   // 7

    // A user-defined (namespaced) count shadow must fall back to the interpreter
    // and return the shadow's value, proving the recognizer never lowers it.
    echo \Shadowed\shadowed_count([1, 2, 3]), "\n";  // 999

    // TypeError: count() of a non-countable scalar throws in PHP 8. The native
    // path side-exits so the interpreter throws the identical error.
    try {
        echo counter(5), "\n";
    } catch (\TypeError $e) {
        echo "TypeError\n";
    }
}
