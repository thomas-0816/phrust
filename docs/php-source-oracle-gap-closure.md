# PHP Source Oracle Gap Closure

This document defines the implementation target for a PHP-source and reference
CLI oracle loop. The loop exists to discover API and runtime-semantic gaps from
the pinned PHP 8.5.7 source and executable reference behavior, then turn those
gaps into deterministic fixtures, reports, and family-level implementation
work. It composes the existing arginfo, stdlib diff, runtime semantics, PHPT,
known-gap, and WordPress smoke tooling instead of replacing it.

The oracle source checkout is read-only input. Local runs may point
`PHP_SRC_DIR` at `/Volumes/CrucialMusic/src/phrust/third_party/php-src` and
`REFERENCE_PHP` at its `sapi/cli/php` binary. Repository defaults may still use
`third_party/php-src` or `third_party/php-src-8.5.7` when present. No tool in
this workflow may vendor, edit, or copy raw upstream tests from `php-src` into
the repository.

## Goals

- Produce a machine-readable compatibility queue from `php-src`, the reference
  CLI, PHPT metadata, existing fixtures, stdlib reports, and application smoke
  traces.
- Generate bounded executable probes that expose whole gap families before real
  applications trip over them.
- Keep WordPress-derived failures as seed patterns only. Fixes must implement
  generic PHP-compatible behavior in the owning engine layer.
- Maintain precise known gaps with stable IDs, concrete fixture or probe
  evidence, layer ownership, source provenance, priority, and reason.
- Keep generated run artifacts under `target/`. Committed summaries must be
  concise and deterministic.

## Existing Inputs

The oracle loop consumes the current project sources of truth:

- `php-src` stubs and source metadata from `PHP_SRC_DIR`, especially extension
  `*.stub.php` files and PHPT corpus metadata.
- The PHP reference CLI from `REFERENCE_PHP` for Reflection,
  `get_defined_functions()`, `get_declared_classes()`, `get_declared_interfaces()`,
  `get_declared_traits()`, `get_defined_constants()`, `get_loaded_extensions()`,
  `php -l`, and executable behavior.
- Generated arginfo from `crates/php_std/src/generated/arginfo.rs`, produced by
  `scripts/stdlib/generate_arginfo.py` and checked by
  `scripts/stdlib/verify_generated_arginfo.sh`.
- Rust standard-library registry JSON from `dump_stdlib_registry`, consumed
  today by `scripts/stdlib/function_coverage.py`.
- Differential stdlib reports from `scripts/stdlib_diff.py`.
- Differential runtime reports from `scripts/runtime_semantics_diff.py` and
  baseline-vs-fast tier reports from `scripts/vm_semantics_oracle.py`.
- PHPT manifests and results from `crates/php_phpt_tools`, `tests/phpt/`, and
  `target/phpt-work/`.
- Runtime semantic fixtures, including `fixtures/runtime_semantics/wp_language_vm`.
- Application smoke traces and heatmaps such as `wp-language-vm`,
  `wp-autoload-stdlib`, `wordpress-real-smoke`, and
  `scripts/wordpress_builtin_heatmap.py`.

## Normalized Outputs

The complete loop produces these normalized artifacts:

- API symbol manifest:
  `target/oracle/api/php-source-api-symbols.jsonl`.
- API summary:
  `target/oracle/api/php-source-api-summary.md`.
- Generated probe files:
  `fixtures/runtime_semantics/oracle_generated/` for committed reduced runtime
  semantics probes, and `target/oracle/probes/` for generated run output.
- Probe manifest:
  `tests/oracle/manifests/generated-probes.jsonl`.
- Probe differential report:
  `target/oracle/probes/oracle-probe-report.json`.
- Merged gap report:
  `target/oracle/gap-report.json`.
- Committed concise gap summary:
  `docs/oracle/gap-report-summary.md`.
- Optional symbol heatmap:
  `target/oracle/symbol-heatmap.json` or a reused
  `target/wordpress-bringup/` heatmap when the input is WordPress-specific.
- Ratchet/check summary:
  `target/oracle/oracle-smoke-summary.json`.
- Next-gap prompt:
  generated on demand by `scripts/oracle/next_gap_prompt.py`.

All JSON and JSONL outputs must be sorted deterministically. Paths in committed
metadata must be repository-relative unless an external oracle path is needed
as provenance.

## Layer Ownership

Each gap row must assign one primary owner:

- `frontend-name-lowering`: namespace imports, resolved class identity,
  PHP-visible class display names, class constants, string interpolation shape,
  and destructuring shape before IR.
- `ir`: expression lowering, lvalue preservation, list/keyed destructuring
  slots, property/dimension fetch ordering, call result assignment, and verifier
  invariants.
- `vm-call-dispatch`: direct calls, closure invocation, callable arrays,
  static callable strings, dynamic static calls, scope/visibility, and
  return-value propagation.
- `vm-reference-model`: references, lvalues, Copy-on-Write, array dimensions,
  object properties, foreach aliases, and by-reference parameter binding.
- `runtime-builtin-api`: builtin execution behavior, deterministic diagnostics,
  side-effect policy, request state, streams/resources, and safe fallback
  behavior for unsupported external APIs.
- `stdlib-metadata-reflection`: generated arginfo, function/class/constant
  registry shape, extension ownership, Reflection metadata, and capability
  introspection.
- `phpt-tooling`: PHPT indexing, source-integrity, generated/minimized PHPT
  fixture provenance, runner policies, and baseline ratchets.

Parser/CST changes are allowed only when syntax shape is missing. Parser code
must not perform name resolution, compile-time semantics, or runtime lowering.

## Seed Pattern Families

The first oracle probe set must cover the current WordPress-derived families as
generic PHP patterns:

- By-reference callback binding through direct calls, closures, callable arrays,
  `call_user_func`, `call_user_func_array`, and array-walk-like APIs.
- Imported class names in class-constant fetches, including values and array
  keys that require PHP-visible fully qualified names.
- List destructuring holes such as `list($a, , $b, $c)`, including nested and
  keyed variants.
- Braced property-dimension interpolation such as
  `"{$this->rewrite['slug']}"`, where property fetch, dimension fetch, and
  string conversion must remain distinct.
- Dynamic static method calls such as `$class::test()` where the class target is
  a runtime value and the call result must remain assignment-compatible.

These families may be seeded by WordPress failures, but the implementation and
fixtures must not special-case WordPress, Requests, or any application path.

## Tooling Plan

The following files and targets are reserved for the next implementation
prompts.

### OP-1A API Index

Add `scripts/oracle/api_index.py`.

Default outputs:

- `target/oracle/api/php-source-api-symbols.jsonl`
- `target/oracle/api/php-source-api-summary.md`

Required row fields:

- `kind`: `function`, `class`, `interface`, `trait`, `enum`, `method`,
  `class_constant`, `constant`, `property`, `ini`, `extension`, or `alias`.
- `name`, `class`, `extension`, `source`, `php_version`, and `provenance`.
- `signature`: parameter names, by-reference flags, variadic flags, optional
  flags, default display, type display, return type, return-by-reference flag,
  tentative flag, and nullable flag when known.
- `visibility`, `static`, `abstract`, `final`, `readonly`, `enum_case`, and
  `autoload_sensitive` where applicable.
- `runtime_value` for constants and class constants when safely obtainable.
- `rust_registry`: `present`, `runtime_builtin`, `class_registered`, and
  `arginfo_present`.
- `status`: `matched`, `missing_in_rust`, `rust_stub`,
  `metadata_mismatch`, `reference_only_known_gap`, `reference_unavailable`, or
  `extractor_gap`.

Targets:

- `oracle-api-index`: build needed helpers and run the indexer.
- `oracle-api-summary`: print the Markdown summary path or render it.

Validation:

- `nix develop -c just oracle-api-index`
- `nix develop -c just verify-generated-arginfo`
- `nix develop -c just stdlib-coverage`
- `nix develop -c cargo test -p php_std`

### OP-1B Probe Generation

Add `scripts/oracle/generate_probes.py`.

Inputs:

- `target/oracle/api/php-source-api-symbols.jsonl` when present.
- Built-in seed metadata or `fixtures/oracle/seeds/*.toml`.

Outputs:

- `fixtures/runtime_semantics/oracle_generated/`
- `tests/oracle/manifests/generated-probes.jsonl`
- `target/oracle/probes/oracle-probe-report.json`

Targets:

- `oracle-probe-generate`
- `oracle-probe-smoke`
- `oracle-probe-full`

The generator should prefer extending `scripts/runtime_semantics_diff.py` and
`scripts/stdlib_diff.py` over creating a duplicate runner. Probe IDs must be
stable across repeated runs, and generated PHP syntax should be linted with
`REFERENCE_PHP` when available.

### OP-1C Gap Report

Add `scripts/oracle/gap_report.py`.

Inputs are optional and composable:

- `target/oracle/api/php-source-api-symbols.jsonl`
- `target/oracle/probes/*report*.json`
- `target/runtime-semantics/*/runtime-semantics-diff-report.json`
- `target/stdlib/*/stdlib-diff-report.json`
- PHPT JSONL results under `target/phpt-work/`
- WordPress smoke reports and extracted first failures
- Existing known-gap catalogs

Outputs:

- `target/oracle/gap-report.json`
- `docs/oracle/gap-report-summary.md`

Classification fields:

- `gap_id`, `status`, `layer`, `pattern_family`, `extension`, `symbol`,
  `source`, `fixture`, `diagnostic_id`, `oracle_reference`, `priority`,
  `confidence`, `suggested_owner`, and `suggested_next_probe`.

Priority:

- `P0`: default execution crash, fatal, OOM/nontermination, uncaught app smoke
  exceptions.
- `P1`: runtime semantic mismatch affecting call dispatch, references/lvalues,
  class/name resolution, include/autoload, constants, or object access.
- `P2`: API surface symbol missing/stub where frameworks commonly probe it.
- `P3`: metadata, reflection, or signature mismatch.
- `P4`: byte-perfect warning text, platform/locale edges, and optional external
  side effects.

Targets:

- `oracle-gap-report`
- `oracle-smoke`: API index, bounded probe smoke, and gap report.
- `verify-oracle`: strict when `REFERENCE_PHP` is configured; explicit skips
  otherwise.

## Ratchet Rules

- New unclassified failures fail `oracle-smoke`.
- Known-gap counts must not increase unless a new row includes a stable ID,
  fixture/probe, source provenance, layer, priority, owner, and reason.
- P0/P1 known gaps must include a reduced fixture and a real-world reproduction
  pointer when available.
- Missing or stub API symbols are queue items unless an explicit extension
  policy marks them unsupported with deterministic behavior.
- The gap report must group related failures into families; it must not emit one
  implementation prompt per raw diff.

## Continuous Workflow

Once OP-1A through OP-1C exist, the normal compatibility workflow is:

1. `PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src
   REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php
   nix develop -c just oracle-smoke`
2. Inspect the top family in `docs/oracle/gap-report-summary.md`.
3. Generate a family-level prompt with `scripts/oracle/next_gap_prompt.py`.
4. Implement the generic layer fix using the existing engine pipeline.
5. Add or promote reduced fixtures/probes with source provenance.
6. Run the focused target for the owner layer.
7. Rerun `oracle-gap-report` and remove or narrow closed known gaps.
8. Run app-level smoke only as final evidence.

`oracle-smoke` may later join `quality-fast`, `verify-runtime`, or
`verify-stdlib` only when it is deterministic and cheap enough for those gates.
Until then it remains a separately advertised compatibility gate.

## Completion Criteria

The whole prompt pack is complete only when:

- `php-src` and the PHP reference CLI produce machine-readable API and behavior
  expectations.
- Missing, stubbed, and mismatched API symbols are visible as a sorted queue.
- Callback/reference/name-resolution/destructuring/interpolation/dynamic-static
  failures have generated and committed reduced fixtures.
- Known gaps are precise and test-backed.
- The gap report shrinks after fixes and a ratchet prevents broad regressions.
- Real application smoke is final compatibility evidence, not the primary
  discovery mechanism.
