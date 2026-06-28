# PHPT Extension Integration Summary

This report defines how extension policy and parallel extension branches merge
without hand-editing full-run state.

## Merge Order

1. `phpt/ext-policy-orchestration`
2. `phpt/ext-text-i18n`
3. `phpt/ext-xml-soap`
4. `phpt/ext-data-platform`
5. final full-regression refresh branch if needed

## Post-Merge Status

The four extension-policy branches are integrated on `main`. Conflict
ownership is resolved by keeping central policy/report structure from
`phpt/ext-policy-orchestration`, text/i18n module behavior from
`phpt/ext-text-i18n`, XML/SOAP module behavior from `phpt/ext-xml-soap`, and
data/platform module policy from `phpt/ext-data-platform`.

No full-baseline acceptance is part of this integration pass. A full regression
run may report new fingerprints, but committed baseline manifests must remain
unchanged unless `PHPT_ACCEPT_BASELINE=1` is explicitly approved.

## Branch Ownership

| Branch | Owned scope | Avoids |
| --- | --- | --- |
| `phpt/ext-policy-orchestration` | Central policy, triage reports, integration reports, module templates, baseline bookkeeping, PHPT tooling. | Extension runtime behavior. |
| `phpt/ext-text-i18n` | `mbstring` and `intl` implementation or stubs, plus focused module docs/manifests. | Central baseline acceptance and unrelated XML/database work. |
| `phpt/ext-xml-soap` | `dom`, `xml`, `simplexml`, `xsl`, and `soap` module work. | Database, PHAR, session, SAPI, and central policy ownership. |
| `phpt/ext-data-platform` | `pdo`, `pdo_sqlite`, `sqlite3`, `mysqli`, `mysqlnd`, `phar`, `session`, `opcache`, and `sapi` policy/module work. | Text/i18n and XML/SOAP implementation details. |

## Pre-Merge Checklist

- Confirm the branch only edits its owned files.
- Confirm no files under `third_party/php-src/` or `target/` are staged.
- Confirm no tokenizer code or tokenizer PHPTs are changed.
- Run the focused module gate named by the branch.
- Run `nix develop -c just verify-phpt`.
- Do not accept a new PHPT baseline unless explicitly instructed.

## Post-Merge Checks

After each branch merge, run:

```bash
nix develop -c just phpt-triage
nix develop -c just phpt-verify-baseline
nix develop -c just verify-phpt
```

After all extension branches merge, run:

```bash
nix develop -c just verify-phpt
nix develop -c just verify-stdlib
nix develop -c just verify-runtime
```

Use `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php`
and `PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src` when
reference-backed checks need the local oracle.

For the repository-local full-regression command, require
`$PWD/third_party/php-src/sapi/cli/php` to exist before running the exact
`REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHPT_RUN_FULL=1` gate.

## Full-Regression Refresh Protocol

Do not run or accept a full baseline in ordinary extension branches. If a final
refresh is explicitly requested:

1. Start from all merged extension branches.
2. Run `nix develop -c just phpt-full-regression` with the required full-run
   environment.
3. Review the raw results under `target/phpt-work/`.
4. Accept updated full-baseline manifests only with explicit approval.
5. Re-run `nix develop -c just phpt-triage`.
6. Re-run `nix develop -c just phpt-verify-baseline`.
7. Re-run `nix develop -c just verify-phpt`.

## Baseline Acceptance Policy

The committed baseline is a no-regression contract. New non-green fingerprints
are not acceptable by default, even if related issues already exist. A baseline
change must include:

- explicit approval to accept the baseline,
- updated full-baseline manifests,
- updated known-gap catalog rows when new stable IDs are needed,
- regenerated reports,
- passing `nix develop -c just phpt-verify-baseline`,
- passing `nix develop -c just verify-phpt`.

## Parallel Branch Sections

### `phpt/ext-text-i18n`

Owns text and internationalization extension behavior. It should use the
extension policy table for `mbstring` and `intl`, then add focused fixtures and
module docs only for those extensions.

### `phpt/ext-xml-soap`

Owns XML-family and SOAP extension behavior. It should keep `soap` out of core
progress unless a focused prompt changes policy, and it should not hand-edit the
central baseline.

### `phpt/ext-data-platform`

Owns data/platform extensions and target policy. It should keep database,
PHAR, session, Opcache, and SAPI decisions separated so core runtime progress is
not blocked by target-specific or service-backed behavior.
