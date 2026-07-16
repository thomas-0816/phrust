# PHPT Source Integrity

The pinned php-src checkout is read-only input. PHPT tools may read Original
PHPT files, source files, generated Reference PHP binaries, and `run-tests.php`,
but they must not edit the checkout or leave generated artifacts inside it.

## Source Tree

The preferred PHPT source path is:

```text
third_party/php-src-8.5.7/
```

The current repository also accepts the existing local path:

```text
third_party/php-src/
```

Commands may override the path with `PHP_SRC_DIR`. A future source-index task
will create `tests/phpt/manifests/php-src-hashes.jsonl` with one JSONL entry per
tracked PHPT or source file.

## Hash Verification

Once the manifest exists, `just phpt-verify-source-integrity` verifies:

- each manifest entry still exists;
- file size and SHA-256 match the recorded values;
- generated run artifacts are not written under the php-src checkout;
- `git -C <php-src> status --short` is empty when the source tree is a Git
  checkout.

A small set of host-generated build artifacts can differ or be absent after a
clean local PHP reference build because bison, re2c, dynasm, or configure
output is toolchain- and platform-sensitive. The verifier reports these
entries with an explicit `[skip]` line and keeps all other manifest entries
strict.

Before the manifest exists, the command still checks the php-src Git status and
reports that hash verification is pending.

## Generated Artifacts

PHPT run output belongs under:

```text
target/phpt-work/
```

`results.jsonl` is strict JSON Lines even when a test prints NUL bytes or other
control characters. The writer escapes those characters and the reader must
round-trip them, so baseline, rerun, and external analysis tools can consume a
complete corpus report without binary-data exceptions.

Do not allow `.diff`, `.out`, `.exp`, `.log`, `.php`, `.clean.php`, `.sh`, or
temporary upload files to accumulate in the Original PHPT corpus.
