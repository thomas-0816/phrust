# WordPress Bootstrap Status

This status page tracks the reduced WordPress runtime blockers that must stay
covered by focused fixtures before a full WordPress bootstrap smoke is useful.

## Covered Canary Gate

Run the reduced blocker suite with:

```bash
nix develop -c env REFERENCE_PHP=/path/to/php-8.5.7/sapi/cli/php just wordpress-blockers
```

The gate uses `scripts/runtime_semantics_diff.py --category wordpress_blockers`
and therefore runs through the same PHP reference / Rust VM executor path as
the existing runtime-semantics differential fixtures. If `REFERENCE_PHP` is not
set, the harness reports the category as skipped instead of treating local Rust
execution as reference proof.

Autoload, feature-detection, callable, and builtin heatmap closure work is
covered separately by:

```bash
nix develop -c env REFERENCE_PHP=/path/to/php-8.5.7/sapi/cli/php just wp-autoload-stdlib
```

That gate writes its runtime-diff report under
`target/runtime-semantics/wp-autoload-stdlib/` and its generated builtin
heatmap under `target/wordpress-bringup/`.

Latest local verification on June 29, 2026 used:

```bash
nix develop -c env REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php just wordpress-blockers
```

Result: 7 fixtures selected, 7 pass, 0 fail, 0 skip, 0 known gaps. The JSON
report was written to
`target/runtime-semantics/wordpress-blockers/runtime-semantics-diff-report.json`.

## Current Blocker Coverage

- Include/require construct operands keep the full concat expression and lower
  to one include instruction.
- Labels and goto lower to normal IR jumps and execute in the VM for reduced
  forward and backward transfers.
- Predefined constants are shared from `php_std` into IR constant folding for
  parameter defaults, property defaults, class constants, and attribute
  arguments.
- Core/path/filesystem/runtime constants used by framework bootstrap probes are
  registered in the standard-library registry and visible through `defined()`,
  including filesystem flags, lock flags, seek/pathinfo/glob/INI/FNM flags,
  JSON option/error constants, HTML entity constants, PCRE flags/errors, and
  the covered date format constants.
- Error-reporting masks preserve bitwise constant-expression behavior.
- Ternary values propagate through explicit IR result blocks.
- `include_once` and `require_once` use canonical paths for repeated relative
  include forms.

## Include Trace Evidence

The include trace can be inspected with:

```bash
target/debug/php-vm run --env PHRUST_TRACE_INCLUDES=1 --env PHRUST_TRACE_RUNTIME=1 --counters-json target/runtime-semantics/wordpress-blockers/include-once-counters.json fixtures/runtime_semantics/wordpress_blockers/include-once-canonical.php
```

The latest trace recorded one canonical `include_once` execution, two once
skips for equivalent paths, `functions=1`, `classes=8`, `constants=0`,
`entry_instructions=7`, included-file `instructions_executed=5`, and total
counter `includes=3`.

## Optional Real WordPress Smoke

After building `target/debug/php-vm`, a local WordPress checkout can be probed
without adding WordPress-specific behavior:

```bash
nix develop -c env REFERENCE_PHP=/path/to/php-8.5.7/sapi/cli/php scripts/runtime_semantics_diff.py --dir /path/to/wordpress --out target/runtime-semantics/wordpress-real --stop-on-fail
```

The report keeps the same stable JSON schema as the reduced canary and includes
failure categories for compile, IR, runtime, error-reporting, predefined
constant, and inclusion/cache failures. Use the first failing file to extract a
reduced fixture before changing runtime behavior.

## Remaining Blockers

No blocker remains in the reduced `wordpress_blockers` canary as of the latest
run above. The full WordPress bootstrap is still expected to expose additional
standard-library, database, HTTP, plugin, theme, and filesystem gaps that are
outside this reduced suite.

## Explicit Non-Goals

The reduced canary suite is not a complete WordPress bootstrap. It does not
claim coverage for WordPress' full standard-library, database, HTTP, plugin,
theme, or filesystem behavior. Wider bootstrap work should add new reduced
fixtures first, then promote a real WordPress smoke only once failures can be
categorized without hiding runtime, optimizer, include-cache, or JIT defects.
