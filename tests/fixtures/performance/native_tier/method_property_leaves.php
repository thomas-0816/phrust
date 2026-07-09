<?php

// Instance-method property leaves: the `$this` mirror of the free-function
// property load/store fixtures. A typed getter `function getN(): int
// { return $this->n; }` and a void setter `function setN($v): void
// { $this->n = $v; }` compile to the same guarded native helper calls, with
// `$this` marshaled into slot 0 (its local in method IR) ahead of the
// declared parameters.
//
// Division of labor on the method path: *untyped-return* trivial accessors
// are already served by the interpreter's trivial-method inliner (direct slot
// access, no frame); the native leaves cover what that inliner rejects — a
// declared scalar return type (with the result-tag guard, so a mismatching
// value side-exits to the interpreter's return-site coercion), a `: void`
// setter, and private/protected properties declared on the receiver class
// itself (their mangled storage never matches the inliner's plain-name slot
// lookup).
//
// The monomorphic class guard pins the method's *declaring* class: a subclass
// instance reaching the same leaf side-exits and interprets. Static methods
// have no `$this`; trait-provided methods (origin != the class) reject at
// recognition.
//
// Differential harness: scripts/performance/copy_patch_native_diff.py runs
// this with the native tier off and on and asserts identical output, and
// against the pinned PHP 8.5.7 reference when available.

class Counter {
    public $n = 0;
    private $secret = 40;
    public $mixed = true; // bool in an untyped slot

    // Typed accessors: rejected by the trivial-method inliner (declared
    // return type), recognized as native leaves.
    public function getN(): int { return $this->n; }
    public function setN($v): void { $this->n = $v; }

    // Private property: only legal from this class's own methods; the leaf
    // reads/writes the mangled storage the interpreter uses.
    public function getSecret(): int { return $this->secret; }
    public function setSecret($v): void { $this->secret = $v; }

    // Result-tag guard: bool in the untyped slot returned through `: int`
    // side-exits; the interpreter's return-site coercion produces int(1).
    public function getMixedAsInt(): int { return $this->mixed; }

    // Untyped-return accessors: the trivial-method inliner serves these; the
    // native recognizer rejects them (no return type -> no tag expectation).
    // Identical behavior either way.
    public function rawN() { return $this->n; }

    // Fluent setter: returns $this, so the void-setter shape rejects; the
    // trivial-method inliner handles it.
    public function withN($v) { $this->n = $v; return $this; }
}

class SubCounter extends Counter {
    public $extra = 9;
}

$c = new Counter();

// Native setter + getter round trip.
$c->setN(7);
var_dump($c->getN());          // int(7)

// Hot loop: repeated native stores and loads through the cached leaves.
for ($i = 0; $i < 1000; $i++) {
    $c->setN($i);
}
var_dump($c->getN());          // int(999)

// Private property through own-class accessors (mangled storage).
var_dump($c->getSecret());     // int(40)
$c->setSecret(41);
var_dump($c->getSecret());     // int(41)

// Result-tag guard: coercion happens in the interpreter, never natively.
var_dump($c->getMixedAsInt()); // int(1)

// Subclass instance on the parent-declared methods: the monomorphic guard
// pins Counter, so SubCounter side-exits and the interpreter runs them.
$sub = new SubCounter();
$sub->setN(11);
var_dump($sub->getN());        // int(11)
var_dump($sub->getSecret());   // int(40)

// Untyped-return accessor + fluent setter (trivial-inliner territory).
var_dump($c->rawN());          // int(999)
var_dump($c->withN(5)->getN()); // int(5)

// Reference-holding slot: the setter must side-exit so the write goes
// through the cell and the alias observes it.
$r = &$c->n;
$c->setN(21);
var_dump($r);                  // int(21)
