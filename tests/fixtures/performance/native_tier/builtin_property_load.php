<?php

// A monomorphic declared-property load lowered to a native guarded helper call —
// the highest-value WordPress copy-patch shape. Inside a leaf that returns one
// declared property of a by-value typed object parameter (`return $o->prop;`),
// the fetch is emitted as an `OpaqueObject`-tag guard plus a `blr` into the
// runtime monomorphic property-load helper over the slot ABI: the object crosses
// as a read-only borrowed handle (an OpaqueObject slot whose payload is a
// `*const Value` valid only for the synchronous call). The helper reuses the
// exact property-load fetch the Cranelift tier uses.
//
// The layout guard is monomorphic: the helper only reads the property when the
// object's runtime class equals the leaf parameter's declared class. A different
// class reaching the same site (a subclass, or any polymorphic instance) fails
// the guard and side-exits, never reading a wrong slot — the interpreter then
// reads the property. Only a *scalar* result (int/bool/float) commits natively;
// a non-scalar value (string/array/object/null), an uninitialized typed
// property, a property with a get/set hook, or a class with a public __get all
// side-exit so the interpreter reproduces the exact value/error. Because the
// native result bypasses the interpreter's return-site coercion, only scalar
// (`int`/`float`/`bool`) and `mixed` return types are recognized, and the
// helper additionally requires the property value to already have exactly the
// declared return type's tag — a mismatching scalar (a `bool` in an untyped
// property returned through `: int`) side-exits so the interpreter coerces or
// errors exactly.
//
// Differential harness: scripts/performance/copy_patch_native_diff.py runs this
// with the native tier off and on and asserts identical output, and against the
// pinned PHP 8.5.7 reference when available.

class Point {
    public int $x = 42;          // scalar int  -> native fast path
    public bool $flag = true;    // scalar bool -> native fast path
    public float $ratio = 1.5;   // scalar float -> native fast path
    public string $label = "hi"; // non-scalar  -> side-exit to interpreter
    public array $items = [1, 2, 3]; // non-scalar -> side-exit to interpreter
    public int $missing;         // typed, uninitialized -> side-exit (throws Error)
    public int $hooked { get => 7; } // property hook -> recognizer rejects it
}

// A subclass with an extra property: a distinct runtime class, so it fails the
// monomorphic layout guard of a leaf typed on the base class.
class Sub extends Point {
    public int $y = 99;
}

// Accessor leaves: each loads the by-value object parameter and returns one
// declared property — the recognized monomorphic property-load shape.
function get_x(Point $o): int { return $o->x; }
function get_flag(Point $o): bool { return $o->flag; }
function get_ratio(Point $o): float { return $o->ratio; }
function get_label(Point $o): string { return $o->label; }
function get_items(Point $o): array { return $o->items; }
function get_missing(Point $o): int { return $o->missing; }
function get_hooked(Point $o): int { return $o->hooked; }

$p = new Point();
$sub = new Sub();

// Scalar reads run natively (int / bool / float).
echo get_x($p), "\n";                          // 42
echo(get_flag($p) ? "true" : "false"), "\n";   // true
echo get_ratio($p), "\n";                      // 1.5

// Non-scalar return types (`: string`, `: array`) are rejected at recognition
// (their values could never commit natively, and richer types have a coercion
// matrix); the interpreter produces the value identically.
echo get_label($p), "\n";                      // hi
echo count(get_items($p)), "\n";               // 3

// Polymorphic 2-class site + subclass-with-extra-prop: get_x is monomorphic on
// Point, so a Sub instance fails the layout guard and side-exits; the
// interpreter reads the inherited property. Identical result either way.
echo get_x($p), "\n";                          // 42 (native)
echo get_x($sub), "\n";                        // 42 (layout-guard side-exit)

// A get-hooked (virtual) property: the recognizer rejects it, so the interpreter
// runs the hook. Identical value either way.
echo get_hooked($p), "\n";                     // 7

// Uninitialized typed property: the layout guard passes, but the property is
// uninitialized, so the helper side-exits and the interpreter throws the exact
// Error.
try {
    echo get_missing($p), "\n";
} catch (\Error $e) {
    echo "Error\n";                            // Error
}

// Return-type coercion: untyped properties can hold any scalar, so the
// helper's result-tag guard is what keeps the native result faithful. A tag
// match commits natively; a mismatch (bool through `: int`, int through
// `: float`) side-exits and the interpreter's return-site coercion produces
// the exact typed value — var_dump pins the type, which echo would hide.
class Loose {
    public $n = 7;    // int in an untyped slot
    public $b = true; // bool in an untyped slot
}

function loose_int(Loose $o): int { return $o->n; }
function loose_bool_as_int(Loose $o): int { return $o->b; }
function loose_int_as_float(Loose $o): float { return $o->n; }

$l = new Loose();
var_dump(loose_int($l));           // int(7) (native: tag match)
var_dump(loose_bool_as_int($l));   // int(1) (side-exit: interpreter coerces)
var_dump(loose_int_as_float($l));  // float(7) (side-exit: interpreter coerces)
