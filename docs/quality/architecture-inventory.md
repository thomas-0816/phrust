# Architecture Inventory

The source-derived architecture inventory is enforced by:

```bash
nix develop -c just architecture-inventory
```

The command classifies tracked Rust files, reports production file size and
workspace dependency edges, records native and platform dependencies, and
counts the public module and re-export surfaces of `php_vm` and `php_runtime`.
It also inventories module-wide Clippy allowances and narrowly identified
source-reparsing, pointer-identity, and diagnostic-string control-flow debt.

Raw source access in semantics and IR lowering is classified in
`scripts/verify/frontend_source_text_inventory.json` as token/literal decoding
(A), structural recovery for missing typed syntax (B), or diagnostic/source-map
rendering (C). The gate rejects unclassified or stale entries and rejects every
category B entry. Category A and C entries therefore document the complete
remaining boundary rather than acting as structural-reparse allowances.

The checked limits live in
`scripts/verify/architecture_inventory_baseline.json`. Findings are identified
by path, category, and normalized source text rather than line number, so moving
code does not hide debt. Removing a finding is allowed; adding one fails the
gate until the architecture is deliberately reviewed and the baseline is
updated.

## Diagnostic control-flow debt

Missing-trait recovery uses the typed `php_ir` lowering payload; include
compilation no longer parses `E_PHP_IR_TRAIT_NOT_FOUND` rendering. The inventory
has no allowance for that pattern, so reintroducing the former prefix parser
fails the gate.

The diagnostic-string parsing and pointer-integer identity baselines are zero.
Include failures, parameter type mismatches, negative string offsets, and
function redeclarations carry typed data across their rendering boundaries.
Arrays and reference cells use process-local logical storage IDs without
exposing allocator addresses or enlarging the runtime `Value` handle.

## Typed frontend boundary

IR lowering no longer reconstructs globals, static locals, construct operands,
dynamic members, default constants, or destructuring shape from raw source.
The source-reparsing baseline is zero. The inventory also rejects the removed
helper families and direct `source_text.slice(...)` calls, while the separate
A/B/C inventory accounts for the remaining metadata, token spelling, and line
number uses.

Reports are generated under `target/architecture/` and remain untracked. To
lower the baseline after a remediation, run:

```bash
nix develop -c scripts/verify/architecture_inventory.py --write-baseline --check
```

Review the baseline diff before committing it. The command list in the report
is derived from currently available `justfile` benchmark targets and is the
starting point for repeatable compile, binary-size, VM, cache, compiler, server,
application, and WordPress measurements.

Capture those measurements with at least three samples per command:

```bash
nix develop -c just architecture-performance-baseline
```

The performance report records median and spread for clean package rebuilds,
incremental root-touch rebuilds, and repository-owned runtime/application
targets. It records peak RSS when `/usr/bin/time` supports it and classifies
optional WordPress measurements as skipped when their prerequisites are absent.
The report and per-run logs are written below
`target/architecture/performance-baseline/`.
