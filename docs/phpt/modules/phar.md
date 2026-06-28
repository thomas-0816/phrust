# phar

- Strategy: classify, no read-only MVP yet
- Classification: real-implementation-required for Composer PHAR mode
- Selected manifest: `tests/phpt/manifests/modules/phar.selected.jsonl`
- Current corpus snapshot: 553 `phar` candidates, 3 PASS, 6 SKIP, 403 FAIL,
  141 BORK, and 552 known non-green outcomes.

## Decision

Do not implement PHAR in this branch.

Composer source mode is the required compatibility path today. Composer PHAR
support is useful later, but a read-only `phar://` MVP must still parse PHAR
archives, expose deterministic wrapper behavior, handle stub/bootstrap rules,
and define a signing policy. The current filesystem/stream support does not
make that a tiny safe patch, so this branch records the policy and keeps
platform probes negative.

## Unsupported Area

- Stable ID: `PHPT-DATA-PHAR`
- Reference behavior: PHP with PHAR enabled exposes `Phar`, `PharData`,
  `PharFileInfo`, archive metadata, stubs, signatures, compression handling,
  and the `phar://` stream wrapper.
- Current phrust behavior: `extension_loaded("phar")`,
  `class_exists("Phar")`, `class_exists("PharData")`, and
  `class_exists("PharFileInfo")` are false. `phar://` is not a supported
  wrapper.
- Fixture: `tests/phpt/generated/phar/platform-checks.phpt`
- Next owner layer: future filesystem/streams plus PHAR archive layer.

## Failure Classification

- Archive object API and metadata PHPTs: `PHPT-DATA-PHAR`.
- `phar://` wrapper PHPTs: `PHPT-DATA-PHAR-WRAPPER`.
- Stub/signature/compression PHPTs: `PHPT-DATA-PHAR-ARCHIVE`.
- Runner BORKs from unsupported sections or source layout remain runner/tooling
  issues and are not accepted as a new baseline by this branch.

## Source References

- `ext/phar/phar_object.stub.php`
- `ext/phar/makestub.php`
- `ext/phar/tests/`

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=phar`
- `nix develop -c just composer-smoke`
- `nix develop -c just verify-phpt`
