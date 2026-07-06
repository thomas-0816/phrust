# Cranelift And Native-Tier Notes

This directory contains Cranelift and native-tier performance documentation.
The native tier is experimental, default-off, and bounded by the performance
optimization gates.

## Contracts And Safety

- [ABI](abi.md)
- [Helper symbol registry](helper-symbol-registry.md)
- [Safety audit](safety-audit.md)
- [Known gaps](known-gaps.md)

## Reports And Tooling

- [Benchmark methodology](benchmark-methodology.md)
- Generated results: `target/performance/cranelift/results.md`
- [JIT report schema](jit-report-schema.md)
- [CLIF dumps](clif-dump.md)
- [Disassembly dumps](disassembly-dumps.md)
- [Framework smokes](framework-smokes.md)
- [No-exec backend](no-exec-backend.md)
