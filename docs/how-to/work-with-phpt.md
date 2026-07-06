# Work With PHPT

PHPT is the primary compatibility workflow for PHP behavior. Original php-src
tests are read-only inputs. Generated, minimized, or derived fixtures live under
`tests/phpt/generated/`, and committed manifests live under
`tests/phpt/manifests/`.

## Bootstrap The Reference Checkout

```bash
nix develop -c just bootstrap-ref
nix develop -c just ref-php-version
```

## Run A Module

```bash
nix develop -c just phpt-module MODULE=operators.conversions
```

For faster iteration after a PHPT binary build:

```bash
nix develop -c just phpt-dev-build
nix develop -c just phpt-dev-module MODULE=operators.conversions
```

Run one file or cluster while debugging:

```bash
nix develop -c just phpt-fast MODULE=operators.conversions FILE=path/to/test.phpt
nix develop -c just phpt-fast MODULE=operators.conversions PATTERN=cast
```

## Rerun Failures

```bash
nix develop -c just phpt-rerun-failures MODULE=operators.conversions
```

## Run The Broad Gate

```bash
nix develop -c just verify-phpt
```

For a full local regression, use the explicit full-run switch:

```bash
PHPT_RUN_FULL=1 nix develop -c just phpt-full-regression
```

## Keep Artifacts In The Right Place

- Original php-src PHPT files stay in the local reference checkout and are never
  edited.
- Generated or minimized committed fixtures live under `tests/phpt/generated/`.
- Manifests and baselines live under `tests/phpt/manifests/`.
- Run artifacts live under `target/phpt-work/` and must not be committed.
- Committed reports under `target/phpt-work/reports/` should be concise summaries, not
  raw runner output.

## Related Docs

- [PHPT guide](../phpt/README.md)
- [Source integrity](../phpt/source-integrity.md)
- [Binary discovery](../phpt/binary-discovery.md)
- [Generated PHPTs](../phpt/generated-tests.md)
- [Full PHPT gate](../phpt/full-phpt-gate.md)
- [Extension policy](../phpt/extension-policy.md)
