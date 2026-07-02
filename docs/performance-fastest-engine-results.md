# Fastest Engine Results

This is a committed summary of the local fastest-engine matrix. The raw
JSON, Markdown, stdout/stderr captures, and counter files are generated
under `target/performance/fastest/` and are not committed.

The matrix is correctness-first. It does not claim that Phrust is the
globally fastest PHP engine; timings are advisory host-local samples over
a bounded fixture corpus.

## Latest Matrix

| Field | Value |
| --- | --- |
| Status | `pass` |
| Fixtures | 12 |
| Enabled rows | 60 |
| Skipped rows | 36 |
| Known-gap rows | 0 |
| Iterations | 1 |
| Warmups | 0 |

## Compared Rows

- `phrust-baseline-ir`
- `phrust-fast-preset`
- `phrust-release-fast`
- `reference-php-cli`
- `reference-php-cli-opcache`

## Explicit Skips

- `phrust-cranelift-optional`: Cranelift row not requested; set PHRUST_FASTEST_MATRIX_JIT=1 or --include-jit
- `phrust-persistent-feedback-optional`: persistent feedback row not requested; set PHRUST_FASTEST_MATRIX_PERSISTENT_FEEDBACK=1 or --include-persistent-feedback
- `phrust-release-pgo`: engine unavailable: target/pgo/php-vm

## Artifacts

- `target/performance/fastest/matrix.json`
- `target/performance/fastest/matrix.md`
- `target/performance/fastest/runs/`

## Policy

- Phrust rows fail if PHP-visible stdout, stderr/runtime diagnostics, or exit status diverge from `phrust-baseline-ir`.
- Reference PHP rows skip cleanly when no local reference binary is available.
- CLI opcache is only reported when the local reference binary accepts the recorded safe INI flags.
- Compile, execution, and total timing fields are separated where Phrust exposes a compile-only command.
