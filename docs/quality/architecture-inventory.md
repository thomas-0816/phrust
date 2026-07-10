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

The remaining allowlisted parsers are narrower legacy rendering boundaries:
typed include failures are owned by Prompt 04, runtime type errors by Prompt 07,
and VM diagnostic construction by Prompt 10. Each baseline entry names its
specific migration owner; new diagnostic-string control flow remains rejected.

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
