# Module Loop Template

Use this template when starting a focused PHPT extension or module loop.

## Module Name

`<module-name>`

## Policy Classification

- Core requirement: `<required-core | optional | out-of-scope>`
- Composer requirement: `<required-composer | optional | out-of-scope>`
- Framework requirement: `<required-framework | optional | out-of-scope>`
- Implementation class: `<stub-only | MVP | real-implementation-required | out-of-scope | already-implemented>`

## Scope

- `<specific behavior this loop owns>`
- `<specific PHPT slice this loop owns>`

## Non-Scope

- No edits to original `php-src` files.
- No generated artifacts under `target/`.
- No tokenizer code or tokenizer PHPT changes unless the current module scope
  explicitly names tokenizer.
- No new PHPT baseline without explicit acceptance.
- No unrelated extension, stdlib, VM, or parser changes.

## Owned Files

- `target/phpt-work/modules/<module>.md`
- `tests/phpt/manifests/modules/<module>.json`
- `tests/phpt/manifests/modules/<module>.selected.jsonl`
- `tests/phpt/generated/<module>/...`
- `<runtime or stdlib files explicitly owned by the module scope>`

## PHPT Manifest Paths

- Corpus: `tests/phpt/manifests/phpt-corpus.jsonl`
- Selected manifest: `tests/phpt/manifests/modules/<module>.selected.jsonl`
- Generated manifest: `tests/phpt/manifests/<module>-generated.jsonl`
- Known gaps: `tests/phpt/manifests/known-gap-catalog.jsonl`

## php-src Oracle Paths

- PHPTs: `$PHP_SRC_DIR/<php-src-path>`
- Stubs or arginfo: `$PHP_SRC_DIR/ext/<extension>/...`
- Source lookup notes: `tests/phpt/manifests/php-src-symbols.jsonl`

Use php-src only as a read-only oracle. Do not copy C implementation into Rust.

## Selected PHPTs

| PHPT | Reason | Expected owner |
| --- | --- | --- |
| `<path>.phpt` | `<why selected>` | `<module/layer>` |

## Generated PHPTs

Every generated PHPT must record provenance:

- original PHPT path,
- original source hash,
- generator version,
- generation timestamp,
- reason for minimization or generation.

## Known Gaps

| Stable ID | Reference behavior | Current phrust behavior | Fixture or PHPT | Next owner layer |
| --- | --- | --- | --- | --- |
| `<id>` | `<reference behavior>` | `<current behavior>` | `<path>` | `<layer>` |

## Local Focused Gates

```bash
nix develop -c just phpt-triage
nix develop -c just phpt-module MODULE=<module>
nix develop -c just verify-phpt
```

Use the narrowest relevant gate while iterating, then run
`nix develop -c just verify-phpt` before finishing.

## Full-Regression Rule

Do not run or accept a full PHPT baseline unless explicitly instructed. If a
full-regression refresh is requested, keep raw artifacts under `target/`, update
the committed baseline manifests only with approval, and re-run:

```bash
nix develop -c just phpt-triage
nix develop -c just phpt-verify-baseline
nix develop -c just verify-phpt
```
