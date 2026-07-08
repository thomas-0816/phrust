<?php

// The pure type predicates is_int/is_string/is_array/is_float/is_bool lowered to
// native tag-check stencils — the copy-and-patch tier's cheapest shape. Inside a
// scalar leaf, is_TYPE($x) needs no helper call: the answer is exactly the tag
// the VM marshaled the argument with (is_int = Int, is_string = OpaqueString,
// is_array = OpaqueArray, is_float = FloatBits, is_bool = Bool). The stencil only
// reads the tag word — it never dereferences the payload.
//
// The one guard: a value the bridge marshals as Uninitialized (null, an object,
// a resource, a reference, …) is ambiguous — the predicate cannot be decided
// from the tag — so the stencil side-exits and the interpreter answers. Every
// definite tag yields a correct true/false natively.
//
// Only the canonical names are lowered; a namespaced shadow keeps its namespace
// in the call name so the recognizer never matches the bare builtin, and the
// aliases (is_integer/is_long/is_double) and non-tag predicates
// (is_null/is_object/is_numeric/…) are left to the interpreter.
//
// Differential harness: scripts/performance/copy_patch_native_diff.py runs this
// with the native tier off and on and asserts identical output, and against the
// pinned PHP 8.5.7 reference when available.

// A namespaced shadow of is_int(): unqualified calls inside the namespace
// resolve to it, and its lowered call name carries the namespace, so the native
// path never matches it — the interpreter runs it, returning the shadow's value.
namespace Shadowed {
    function is_int($x): bool {
        return true; // always true, unlike the real is_int
    }

    function shadowed_is_int($x): bool {
        return is_int($x);
    }
}

namespace {
    // The native type-predicate leaves: each is_TYPE($x) on a by-value parameter.
    function t_is_int($x): bool {
        return is_int($x);
    }
    function t_is_string($x): bool {
        return is_string($x);
    }
    function t_is_array($x): bool {
        return is_array($x);
    }
    function t_is_float($x): bool {
        return is_float($x);
    }
    function t_is_bool($x): bool {
        return is_bool($x);
    }

    function p(bool $v): string {
        return $v ? "T" : "F";
    }

    // Every definite category plus the ambiguous null/object (which side-exit to
    // the interpreter). Each predicate is applied across all of them, so we cover
    // matching and non-matching tags and the Uninitialized side exit.
    $values = [7, "hi", [1, 2], 1.5, true, null, new stdClass()];

    foreach ($values as $v) {
        echo p(t_is_int($v));
    }
    echo "\n"; // TFFFFFF

    foreach ($values as $v) {
        echo p(t_is_string($v));
    }
    echo "\n"; // FTFFFFF

    foreach ($values as $v) {
        echo p(t_is_array($v));
    }
    echo "\n"; // FFTFFFF

    foreach ($values as $v) {
        echo p(t_is_float($v));
    }
    echo "\n"; // FFFTFFF

    foreach ($values as $v) {
        echo p(t_is_bool($v));
    }
    echo "\n"; // FFFFTFF

    // A namespaced is_int shadow falls back to the interpreter and returns the
    // shadow's value (always true), proving the recognizer never lowers it.
    echo p(\Shadowed\shadowed_is_int("not an int")), "\n"; // T
}
