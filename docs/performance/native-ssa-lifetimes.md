# Executable native SSA and value lifetimes

The optimizing Cranelift tier consumes PHP-specific value-flow facts built from
the authoritative executable `RegionGraph`. Cranelift variables remain the
machine-SSA and phi implementation; the Region analysis supplies the value
class, certainty, local storage class, and ownership facts that decide whether
an operation can stay in registers.

Current direct cases include initialized scalar locals, immortal constants,
integer add/subtract/multiply and bitwise operations, integer comparisons,
null/bool/int truthiness, and simple scalar casts. Checked arithmetic enters the
typed numeric helper only on overflow. References, top-level globals,
superglobals, `GLOBALS`, suspension-persistent locals, unknown classes, and
observable PHP conversions remain explicit runtime boundaries.

Boolean results use reserved immutable native constant handles. This preserves
PHP boolean identity when direct CLIF comparisons and casts return to runtime
code; raw `0` and `1` continue to mean PHP integers.

Every stable helper has an ownership contract describing borrowed/consumed
inputs, result ownership, and possible input aliasing. Scalar and immortal
copies use no retain/release helper. Runtime handles keep the conservative
boundary until last-use ownership proves a move or a balanced lifetime.

Object release uses a request-local root index. Published root mutations dirty
one generation; the next non-unique object release rebuilds reachable object
membership once, and later releases use constant-time membership until another
semantic mutation boundary.

Validation:

```bash
nix develop -c cargo test -p php_jit
nix develop -c cargo test -p php_vm
nix develop -c just optimizer-diff
nix develop -c just native-ssa-ratchet
nix develop -c just verify-performance
```

`just native-ssa-report` renders the required B9 evidence bundle under
`target/post-cutover/ssa-lifetimes/`. Its default input is deliberately marked
as a validation fixture. Only rerun it with `--after-kind wordpress` and clean
benchmark JSON from the same environment when the WordPress prerequisites are
available; fixture counts must not be presented as tranche acceptance.
