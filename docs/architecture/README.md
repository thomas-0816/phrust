# Architecture Documentation

This index links the implementation contracts that contributors should read
before changing a subsystem.

- [ADRs](../adr/README.md): accepted architecture decisions.
- [Lexer architecture](../lexer/lexer-architecture.md) and
  [token model](../lexer/token-model.md).
- [Parser architecture](../parser/parser-architecture.md),
  [CST model](../parser/cst-model.md), and
  [lexer/parser boundary](../parser/lexer-parser-boundary.md).
- [Semantic frontend architecture](../frontend/semantic-frontend-architecture.md),
  [HIR model](../frontend/hir-model.md), and
  [declaration model](../frontend/declaration-model.md).
- [Runtime and VM](../runtime/README.md), including
  [runtime values](../runtime/values.md),
  [VM structure](../runtime/vm-structure.md), and
  [runtime cache architecture](../runtime/cache-architecture.md), plus the
  [include subsystem ownership contract](include-subsystem.md).
- [Standard library](../stdlib/README.md) and
  [builtin modules](../runtime/builtin-modules.md).
- [Server architecture](../server-architecture.md) and
  [server functionality](../server-functionality.md).
- [Performance](../performance/README.md), including
  [methodology](../performance/methodology.md),
  [native optimization gates](../performance/optimization-gates.md), and
  [native compile cache](../performance/native-compile-cache.md).

Architecture docs describe stable contracts and boundaries. Implementation
history, one-off benchmark captures, and local run evidence belong under
`target/` or in a short-lived issue/PR discussion.

## Additional References

- [Architecture Guardrails](guardrails.md)
