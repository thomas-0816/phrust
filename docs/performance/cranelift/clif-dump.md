# Performance Cranelift CLIF Dump

A later stage adds a standalone Cranelift IR smoke that does not consume PHP
IR and does not execute native code. The smoke builds one deterministic
function:

```text
fn(i64, i64) -> i64
```

The function adds its two parameters and returns the result. It is intentionally
small so the dump proves only that the Cranelift frontend and verifier are
wired into the optional `jit-cranelift` feature.

## Command

```bash
nix develop -c just dump-cranelift-clif
```

The command runs `php-vm dump-cranelift-clif` with the `jit-cranelift` feature
enabled and writes:

```text
target/performance/cranelift/trivial_add.clif
```

`target/` output is generated evidence and must not be committed.

## Reading The Dump

The first line contains the Cranelift function signature. For this smoke it is
expected to contain two `i64` parameters and one `i64` return. The body should
contain:

- block parameters for the two function inputs,
- one `iadd` instruction,
- one `return` instruction.

The command fails if the Cranelift verifier rejects the generated function.
Because this smoke is standalone, a passing dump does not prove PHP IR lowering,
JIT eligibility, native execution, side exits, helper calls, or runtime
correctness. Those are later addendum work items.
