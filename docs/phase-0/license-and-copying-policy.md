# License and Copying Policy

This document is an engineering policy for Phase 0. It is not legal advice.

## Reference Use

`php-src` is used as the pinned reference oracle for PHP `8.5.7`. The local
checkout lives under:

```text
third_party/php-src
```

That checkout is ignored by Git and must not be committed as a vendored source
copy.

## Allowed Phase 0 Artifacts

Phase 0 may store:

- Paths to reference files.
- Hashes.
- File sizes.
- Line counts.
- Git commit and tag metadata.
- Test specifications.
- Self-written descriptions of observed behavior and planned gates.

These artifacts belong under `references/`, `docs/`, scripts, or future
generated metadata paths.

## Not Part of Phase 0

Directly copying larger code blocks from `php-src` into the Rust engine is not
part of Phase 0. Any later code or test import from `php-src` needs an explicit
license and provenance review.

If later phases import code, tests, or substantial derived material from
`php-src`, the original license notices and provenance must be preserved.

## Checklist

- Was code copied?
- Is the source license named?
- Is provenance documented?
- Is copying actually necessary?
- Can the same purpose be served with paths, hashes, metadata, or generated
  expectations instead?

## Phase 0 Rule

No vendored `php-src` copy is committed in Phase 0.
