# PHPT Status

PHPT is the primary compatibility loop for runtime and standard-library work.
The committed source of truth is the PHPT manifest and known-gap data under
`tests/phpt/manifests/`, plus the human workflow docs under `docs/phpt/`.

## Current Contract

- Original php-src PHPT files are read-only inputs.
- Generated and minimized Phrust-owned PHPTs live under
  `tests/phpt/generated/`.
- Full-run baselines and known-gap catalogs live under
  `tests/phpt/manifests/`.
- Local run results, triage reports, and full-baseline markdown reports are
  generated under `target/phpt-work/`.

## Main Commands

```bash
nix develop -c just phpt-runner-smoke
nix develop -c just phpt-dev-module MODULE=standard.strings
PHPT_RUN_FULL=1 nix develop -c just phpt-full-regression
nix develop -c just phpt-verify-baseline
```

Use [Work with PHPT](../how-to/work-with-phpt.md) for the iteration workflow
and [Full PHPT gate](../phpt/full-phpt-gate.md) for baseline policy.
