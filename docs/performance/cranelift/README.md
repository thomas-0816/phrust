# Cranelift And Native-Tier Notes

This directory contains Cranelift and native-tier performance documentation.
The native tier is experimental, default-off, and bounded by the performance
optimization gates.

## Contracts And Safety

- [ABI](abi.md)
- [Helper symbol registry](helper-symbol-registry.md)
- [Safety audit](safety-audit.md)
- [Process code manager](code-manager.md)
- [Cranelift-only cutover](cutover.md)
- [AMD64 scalar regions, dynamic tiering, and bounded prewarm](amd64.md#scalar-regions-and-calls)
- [Known gaps](known-gaps.md)

## Reports And Tooling

- [AMD64 Linux build, smoke, and WordPress A/B](amd64.md)
- [Benchmark methodology](benchmark-methodology.md)
- Generated results: `target/performance/cranelift/results.md`
- [JIT report schema](jit-report-schema.md)
- [CLIF dumps](clif-dump.md)
- [Disassembly dumps](disassembly-dumps.md)
- [Framework smokes](framework-smokes.md)
- [No-exec backend](no-exec-backend.md)
