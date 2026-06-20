# ADR 0004: No Vendored php-src

## Status

Accepted

## Context

The project needs `php-src` as a reference oracle, but committing a full source
copy would increase repository size and create license/provenance complexity.
Phase 0 only needs a local checkout, lockfile, and metadata.

## Decision

Do not vendor `php-src` into this repository.

The local checkout lives under:

```text
third_party/php-src
```

That path is ignored by Git. Reference metadata, paths, hashes, and lockfiles
live under `references/`.

## Consequences

- Developers bootstrap the reference with `nix develop -c just bootstrap-ref`.
- CI required checks do not need to clone `php-src`.
- Reference source updates are represented through lockfile and ADR changes.
- Later source or test imports require explicit license and provenance review.

## Alternatives

- Commit a vendored `php-src` copy. Rejected because it is large and creates
  unnecessary provenance risk.
- Use a Git submodule. Deferred; it adds workflow complexity and is not needed
  for Phase 0.
- Use a source tarball with a checksum. Deferred; it is viable but less
  convenient for metadata extraction and source inspection.

## Date

2026-06-19
