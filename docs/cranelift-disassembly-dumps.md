# Phase 7 Cranelift Disassembly And Code-Size Dumps

Optional prompt 07.CL.B adds a local diagnostic dump for Cranelift-generated
code. It is deliberately outside the default Phase 7 and Cranelift verification
gates because native disassembly is architecture-specific and should not decide
CI success.

## Command

```bash
nix develop -c just jit-cranelift-disasm
```

The command builds `php-vm` with `jit-cranelift`, runs a bounded set of
Cranelift Big-Win fixtures with `--jit=cranelift --jit-eager --jit-stats=json`
and `--jit-dump-clif`, and writes all outputs under:

```text
target/phase7/cranelift/disasm/
```

Generated files include:

- `manifest.json`: machine-readable index of every emitted dump.
- `<scenario>.json`: one descriptor per compiled scenario.
- `<scenario>.clif`: the Cranelift IR dump produced by `--jit-dump-clif`.
- `<scenario>.disasm.txt`: a human-readable diagnostic summary.

`target/` output is generated evidence and must not be committed.

## Descriptor Fields

Each descriptor links the diagnostic artifacts to the VM compile-cache identity:

- `function_id`: the `FunctionId` used for the compile request.
- `function_name`: the IR function name.
- `ir_fingerprint`: the stable fingerprint already used in the process-local
  JIT compile-cache key.
- `code_bytes`: native code bytes reported by Cranelift for that function.
- `compile_time_nanos`: compile latency for that function.
- `target_isa`, `abi_hash`, and `config_hash`: cache-key fields used to avoid
  mixing incompatible native code.
- `clif_dump`: the CLIF artifact used for inspection.

## Native Disassembly Status

Phase 7 uses Cranelift `JITModule`, so native code is owned by the in-process
JIT allocation. This repo does not expose an object file or a safe JIT-memory
extraction API for `objdump`. The diagnostic therefore records
`native_disassembly_status: skipped` and reports code size plus CLIF instead of
pretending to disassemble bytes it cannot safely materialize.

The Optional 07.CL.A ObjectModule research documents what would be needed for a
future object-file path. Until that path exists, this dump is for local
performance diagnosis only and does not influence runtime behavior, tiering,
eligibility, or CI gates.
