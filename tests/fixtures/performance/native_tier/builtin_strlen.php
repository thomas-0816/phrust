<?php

// Builtin strlen() lowered to a native helper call — the second heap-handle
// shape of the copy-and-patch native tier, mirroring count(). Inside a scalar
// leaf, strlen($s) is emitted as a real `blr` into the VM's string-length ABI
// wrapper over the slot ABI: the string crosses as a read-only borrowed handle
// (an OpaqueString slot whose payload is a `*const Value` valid only for the
// synchronous call). The helper reads its BYTE length — PHP strlen is a byte
// count, not a multibyte length.
//
// Only a genuine string runs natively. Every other value — an int/float/bool the
// interpreter would coerce, or an array/null/object — trips the string-tag guard
// and side-exits, so the interpreter reproduces the exact semantics. A
// user-defined (namespaced) strlen shadow is never lowered: its call name
// carries the namespace, so the recognizer never matches the bare builtin.
//
// The native path is only taken after the VM confirms `strlen` resolves to the
// real builtin (see NativeCallPermits::builtin_strlen), mirroring count()/abs().
//
// Differential harness: scripts/performance/copy_patch_native_diff.py runs this
// with the native tier off and on and asserts identical output, and against the
// pinned PHP 8.5.7 reference when available.

// A namespaced shadow of strlen(): calls inside the namespace resolve to this
// function, and its lowered call name carries the namespace, so the native path
// never matches it — the interpreter runs it, returning the shadow's value.
namespace Shadowed {
    function strlen($ignored): int {
        return 999;
    }

    function shadowed_strlen($s): int {
        return strlen($s);
    }
}

namespace {
    // The native strlen leaf: strlen($s) on a by-value parameter.
    function len($s): int {
        return strlen($s);
    }

    // Genuine strings run natively; the helper reads the byte length.
    echo len("hello"), "\n";              // 5   (ASCII)
    echo len(""), "\n";                   // 0   (empty)
    echo len("a\0b"), "\n";               // 3   (embedded NUL byte)
    echo len("héllo"), "\n";              // 6   (é is two UTF-8 bytes, byte count)

    // Non-string arguments side-exit at the tag guard; the interpreter applies
    // strlen's coercion, matching PHP exactly.
    echo len(123), "\n";                  // 3   (int 123 -> "123")
    echo len(45.5), "\n";                 // 4   (float 45.5 -> "45.5")

    // A user-defined (namespaced) strlen shadow must fall back to the interpreter
    // and return the shadow's value, proving the recognizer never lowers it.
    echo \Shadowed\shadowed_strlen("hello"), "\n"; // 999
}
