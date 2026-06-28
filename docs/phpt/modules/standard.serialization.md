# standard.serialization

- Priority: 16
- Selected manifest: `tests/phpt/manifests/modules/standard.serialization.selected.jsonl`
- Prompt 16.1 baseline: 16 PASS, 2 SKIP, 107 FAIL, 0 BORK from 126 corpus candidates
- Prompt 16.9 focused gate: 5 PASS, 0 FAIL, 0 BORK

## Scope

- `serialize`
- `unserialize`
- Scalar, array, and simple object persistence covered by the selected gate

## Non-Scope

- Session module persistence
- PHP `R`/`r` reference identity records
- Full magic hook and resource serialization behavior

## Relevant PHPT Paths

- `ext/standard/tests/serialize/002.phpt`
- `ext/standard/tests/serialize/004.phpt`
- `tests/phpt/generated/standard.serialization/serialize-unserialize-scalars-arrays.phpt`
- `tests/phpt/generated/standard.serialization/serialize-unserialize-simple-object.phpt`
- `tests/phpt/generated/standard.serialization/unserialize-reference-record-gap.phpt`

## Relevant Source Areas

- `crates/php_runtime/src/serialization.rs`
- `crates/php_runtime/src/value.rs`
- `crates/php_runtime/src/object/`
- `docs/stdlib-serialization.md`

## Target Gates

- `nix develop -c cargo test -p php_runtime serialization -- --nocapture`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=standard.serialization`
- `nix develop -c just verify-stdlib`

## Prompt 16 Evidence

- Narrowed the selected manifest to scalar/array/simple-object serialization
  plus an explicit reference-record known-gap fixture.
- Documented that `R`/`r` reference identity records are intentionally rejected
  as `STDLIB-GAP-SERIALIZE-REFERENCES`.
- Latest focused target run: PASS, 5 selected PHPTs.

## Known Gaps

- `R`/`r` reference identity records are not emitted or reconstructed.
- `allowed_classes`, magic hooks, resources, and deep object/reference graphs
  remain outside the Prompt 16 focused gate.
