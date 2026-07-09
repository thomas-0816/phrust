<?php

// A monomorphic declared-property *store* lowered to a native guarded helper
// call — the write-side mirror of builtin_property_load.php. Inside a void
// setter leaf that assigns its untyped by-value parameter to one declared
// *untyped* property of a by-value typed object parameter
// (`$o->prop = $v;`), the assignment is emitted as an `OpaqueObject`-tag
// guard plus a `blr` into the runtime monomorphic property-store helper over
// the slot ABI: the object crosses as a borrowed handle, the new value as the
// address of its marshaled slot, and the helper commits exactly one
// name-keyed storage write through the runtime's own interior-mutability
// layer.
//
// The layout guard is monomorphic: the helper only writes when the object's
// runtime class equals the leaf parameter's declared class. The write only
// proceeds when the slot currently holds a plain initialized value — a
// reference-holding slot (the write must go through the cell so aliases
// observe it), an absent slot (`unset()` re-arms dynamic/`__set` semantics),
// or a class mismatch all side-exit *before any write*, so the interpreter
// performs the exact store. Only a marshaled scalar (int/bool/float) value
// commits natively; strings, arrays, and null side-exit. Typed, readonly,
// hooked, and asymmetric-visibility (`private(set)`) properties — and typed
// value parameters, whose bind-time coercion the native path would skip — are
// rejected at recognition time, so those functions always interpret.
//
// Differential harness: scripts/performance/copy_patch_native_diff.py runs
// this with the native tier off and on and asserts identical output, and
// against the pinned PHP 8.5.7 reference when available.

class Box {
    public $val = 0;                 // untyped -> native store fast path
    public int $typed = 1;           // typed -> recognizer rejects (coercion/TypeError interpreted)
    public readonly int $ro;         // readonly -> recognizer rejects (write throws)
    public private(set) int $ps = 3; // asymmetric visibility -> recognizer rejects (write throws)
    public int $hookset { set => $value * 2; }     // set hook -> recognizer rejects (hook runs)
}

// A subclass: a distinct runtime class, so it fails the monomorphic layout
// guard of a leaf typed on the base class and the interpreter writes the
// inherited property.
class SubBox extends Box {
    public $extra = 9;
}

// Setter leaves: each assigns the untyped by-value parameter to one declared
// property of the typed object parameter — the recognized store shape (only
// set_val targets an eligible untyped plain property).
function set_val(Box $o, $v): void { $o->val = $v; }
function set_typed(Box $o, $v): void { $o->typed = $v; }
function set_ro(Box $o, $v): void { $o->ro = $v; }
function set_ps(Box $o, $v): void { $o->ps = $v; }
function set_hookset(Box $o, $v): void { $o->hookset = $v; }
// A *typed* value parameter coerces at bind time (7.0 -> int 7); the
// recognizer rejects it so the interpreter's coercion always applies.
function set_val_typed_param(Box $o, int $v): void { $o->val = $v; }

$a = new Box();
$sub = new SubBox();

// Scalar stores run natively (int / bool / float), including overwriting a
// non-scalar previous value with a scalar.
set_val($a, 7);
echo $a->val, "\n";                            // 7
set_val($a, true);
echo($a->val ? "true" : "false"), "\n";        // true
set_val($a, 2.5);
echo $a->val, "\n";                            // 2.5

// Non-scalar new values side-exit at the scalar-value gate; the interpreter
// stores them identically.
set_val($a, "str");
echo $a->val, "\n";                            // str
set_val($a, [1, 2]);
echo count($a->val), "\n";                     // 2
set_val($a, null);
echo var_export($a->val, true), "\n";          // NULL

// Hot loop: repeated native stores through the cached leaf (the last write
// wins; the previous value was non-scalar, overwritten natively).
for ($i = 0; $i < 1000; $i++) {
    set_val($a, $i);
}
echo $a->val, "\n";                            // 999

// Subclass instance: fails the monomorphic layout guard and side-exits; the
// interpreter writes the inherited property.
set_val($sub, 11);
echo $sub->val, "\n";                          // 11

// Reference-holding slot: the store must go through the reference cell so the
// alias observes it — the storage guard side-exits before any write and the
// interpreter assigns through the cell.
$c = new Box();
$r = &$c->val;
set_val($c, 21);
echo $r, "\n";                                 // 21

// unset() slot: absent storage side-exits (a direct write would skip the
// dynamic-property re-creation path); the interpreter re-creates it.
$d = new Box();
unset($d->val);
set_val($d, 31);
echo $d->val, "\n";                            // 31

// Typed property: never native; the interpreter enforces the declared type
// (a matching int stores, a non-numeric string throws TypeError).
$e = new Box();
set_typed($e, 5);
echo $e->typed, "\n";                          // 5
try {
    set_typed($e, "abc");
} catch (\TypeError $t) {
    echo "TypeError\n";                        // TypeError
}

// Typed *value parameter*: bind-time coercion turns 7.0 into int 7 — the
// recognizer rejects this shape, so the coercion always applies (a native
// store of the raw argument would leave a float here).
set_val_typed_param($e, 7.0);
echo is_int($e->val) ? "int" : "float", "\n";  // int
echo $e->val, "\n";                            // 7

// readonly / private(set): never native; the interpreter throws the exact
// Error for a write from a free function.
try {
    set_ro($e, 1);
} catch (\Error $err) {
    echo "Error\n";                            // Error
}
try {
    set_ps($e, 4);
} catch (\Error $err) {
    echo "Error\n";                            // Error
}

// A set-hooked property: the recognizer rejects it, so the interpreter runs
// the assignment (native off and on are identical). The backing-store
// read-back is not asserted here: materializing a `set`-hook write is a
// pre-existing runtime hook gap (reference stores 10, current runtime does
// not persist it), tracked in the PHPT baseline — this fixture only pins the
// native tier's behavior, which must not change whatever the hook does.
set_hookset($e, 5);
echo "hookset-call-ok\n";                      // hookset-call-ok
