# Cranelift Polymorphic Inline Cache Research for Phase 7

Optional 07.CL.E investigates whether Cranelift should eventually consume
property and method inline-cache metadata beyond the current monomorphic fast
paths. It does not enable polymorphic dispatch in production, CI, or the
default Cranelift verification gate.

## States

Phase 7 already records property and method callsite profiles with three
receiver-shape states:

- `monomorphic`: one receiver class has been observed. This is the only state
  used by the existing Cranelift property-load and direct-method fast paths.
- `polymorphic`: two to four receiver classes have been observed. A future JIT
  path could emit a short receiver-class guard chain and then dispatch to the
  matching property slot or method target.
- `megamorphic`: more receiver classes have been observed than the configured
  polymorphic cap. The JIT should avoid compiling an ever-growing guard chain
  and should use the generic VM path.

The local experiment uses a hard cap of four entries. That keeps the prototype
inside the requested 2-4 entry range while preserving a simple audit rule:
fixtures with five receiver classes must report `megamorphic_fallback`.

## Experimental-Off Prototype

`scripts/phase7/cranelift/polymorphic_ic_experiment.py` generates local PHP
fixtures under `target/phase7/cranelift/polymorphic-ic/fixtures/` and runs each
fixture with `--jit=off` and eager `--jit=cranelift`. The script consumes the
existing VM counter JSON profile arrays:

- `property_fetch_profiles`
- `method_call_profiles`

It then derives a hypothetical guard plan from the recorded metadata:

- receiver class id and receiver class name;
- property slot and layout version for property reads;
- method id, method slot, and override layout version for method calls;
- capped guard entry count;
- generic fallback on guard miss for polymorphic sites;
- explicit `megamorphic_fallback` once the cap is exceeded.

The prototype only writes reports. It does not add a new runtime dispatch path,
does not change Cranelift lowering, and does not make polymorphic property or
method calls JIT-eligible.

## Local Fixture Benchmark

The optional local target is:

```bash
nix develop -c just jit-cranelift-poly-ic-experiment
```

It writes:

- `target/phase7/cranelift/polymorphic-ic/report.json`;
- `target/phase7/cranelift/polymorphic-ic/guard-report.json`;
- `target/phase7/cranelift/polymorphic-ic/guard-report.txt`;
- generated fixtures and counter JSON under the same `target/` subtree.

The target is intentionally excluded from `just verify-phase7-cranelift`.
Framework-independent local fixtures are enough for this optional research
prompt; making it a default gate would imply a runtime commitment Phase 7 does
not make.

## Recommendation

Do not enable polymorphic JIT inline caches by default in Phase 7. Keep the
existing monomorphic fast paths as the executable subset and use this report to
size a future implementation. A later phase can revisit polymorphic Cranelift
guards after it has:

- stable deoptimization and side-exit state for property and method guards;
- broader framework-like evidence that polymorphic sites dominate hot paths;
- compile-budget rules for guard-chain size;
- clear invalidation rules for class layout and method-cache epochs;
- negative fixtures for magic methods, hooks, dynamic properties, visibility,
  references, named arguments, unpacking, and by-reference parameters.

Until then, megamorphic sites must remain generic VM fallbacks, and the report
must keep showing that behavior explicitly.
