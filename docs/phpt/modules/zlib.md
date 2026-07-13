# zlib PHPT Coverage

## Strategy

The zlib slice keeps the existing `flate2` backend and narrows promotion to
PHP-visible wrapper behavior proven against the php-src PHPT oracle. Selected
rows cover compression roundtrips, deflate/inflate contexts, gzip-backed stream
resources, selected output-compression header/no-op rows, and target-green
gzip cursor/write variations. The current slice also covers the built-in
`zlib.deflate` and `zlib.inflate` stream filters on memory-backed streams.

## Selected Rows

The selected manifest currently contains 56 rows.

- `tests/phpt/generated/zlib/compression-basic.phpt`
- `tests/phpt/generated/zlib/gzip-stream-helpers.phpt`
- `tests/phpt/generated/zlib/stream-filters.phpt`
- `ext/zlib/tests/gzcompress_basic1.phpt`
- `ext/zlib/tests/gzdeflate_basic1.phpt`
- `ext/zlib/tests/gzdeflate_variation1.phpt`
- `ext/zlib/tests/gzencode_basic1.phpt`
- `ext/zlib/tests/gzuncompress_basic1.phpt`
- `ext/zlib/tests/gzclose_basic.phpt`
- `ext/zlib/tests/gzgetc_basic.phpt`
- `ext/zlib/tests/gzgets_basic.phpt`
- `ext/zlib/tests/gzread_basic.phpt`
- `ext/zlib/tests/gzrewind_basic.phpt`
- `ext/zlib/tests/gzseek_basic.phpt`
- `ext/zlib/tests/gztell_basic.phpt`
- `ext/zlib/tests/gzfile_basic.phpt`
- `ext/zlib/tests/readgzfile_basic.phpt`
- `ext/zlib/tests/zlib_get_coding_type_basic.phpt`
- `ext/zlib/tests/deflate_add_basic.phpt`
- `ext/zlib/tests/deflate_init_reuse.phpt`
- `ext/zlib/tests/inflate_get_status.phpt`
- `ext/zlib/tests/inflate_get_read_len.phpt`
- `ext/zlib/tests/inflate_init_reuse.phpt`
- `ext/zlib/tests/008.phpt`
- `ext/zlib/tests/bug51269.phpt`
- `ext/zlib/tests/bug61287.phpt`
- `ext/zlib/tests/bug61443.phpt`
- `ext/zlib/tests/bug74240.phpt`
- `ext/zlib/tests/bug75299.phpt`
- `ext/zlib/tests/gzcompress_variation1.phpt`
- `ext/zlib/tests/gzeof_basic.phpt`
- `ext/zlib/tests/gzfile-mb.phpt`
- `ext/zlib/tests/gzfilegzreadfile.phpt`
- `ext/zlib/tests/gzgetc_basic_1.phpt`
- `ext/zlib/tests/gzopen_basic.phpt`
- `ext/zlib/tests/gzopen_variation6.phpt`
- `ext/zlib/tests/gzopen_variation7.phpt`
- `ext/zlib/tests/gzpassthru_basic.phpt`
- `ext/zlib/tests/gzputs_basic.phpt`
- `ext/zlib/tests/gzread_variation1.phpt`
- `ext/zlib/tests/gzreadgzwrite.phpt`
- `ext/zlib/tests/gzrewind_basic2.phpt`
- `ext/zlib/tests/gzseek_basic2.phpt`
- `ext/zlib/tests/gzseek_seek_oob.phpt`
- `ext/zlib/tests/gzseek_variation1.phpt`
- `ext/zlib/tests/gzseek_variation2.phpt`
- `ext/zlib/tests/gzseek_variation3.phpt`
- `ext/zlib/tests/gzseek_variation4.phpt`
- `ext/zlib/tests/gzseek_variation5.phpt`
- `ext/zlib/tests/gztell_basic2.phpt`
- `ext/zlib/tests/gzwrite_basic.phpt`
- `ext/zlib/tests/gzwrite_error2.phpt`
- `ext/zlib/tests/ob_002.phpt`
- `ext/zlib/tests/zlib_get_coding_type_br.phpt`
- `ext/zlib/tests/zlib_wrapper_fflush_basic.phpt`
- `tests/basic/req44164.phpt`

## Implemented Surface

- `gzencode`, `gzdecode`, `gzcompress`, `gzuncompress`, `gzdeflate`,
  `gzinflate`, `zlib_encode`, and `zlib_decode`.
- Gzip file resource helpers including `gzopen`, `gzread`, `gzgetc`,
  `gzgets`, `gzwrite`, `gzclose`, `gzrewind`, `gzseek`, `gztell`,
  `gzfile`, and `readgzfile`.
- `zlib_get_coding_type()` returns `false` in the current runtime because
  SAPI output compression is not implemented.
- `deflate_init`, `deflate_add`, `inflate_init`, `inflate_add`,
  `inflate_get_status`, and `inflate_get_read_len` support the selected
  php-src context PHPTs, including context reuse after `ZLIB_FINISH` and
  zlib-wrapped inflate read-length/status tracking.
- Closed gzip resources now raise the PHP-compatible open-stream `TypeError`
  for gzip helper calls.
- `gzgets($stream, $length)` now consumes and returns at most `length - 1`
  bytes, matching `fgets` cursor behavior.
- `STREAM_FILTER_READ`, `STREAM_FILTER_WRITE`, and `STREAM_FILTER_ALL` are
  registered, and `stream_filter_append`, `stream_filter_prepend`,
  `stream_filter_remove`, and the unsupported-user-filter failure path for
  `stream_filter_register` are available.
- Built-in `zlib.deflate` write filters and `zlib.inflate` read filters are
  backed by `flate2` and covered for append/prepend, removal, and unknown-filter
  failure on `php://temp` streams.
- Selected output-compression rows cover disabled/no-op compression behavior,
  Vary/Content-Encoding header preservation, Content-Length output, and
  unsupported `br` coding-type fallback. They do not prove full SAPI output
  compression parity.

## Current Gate

The selected zlib module gate is policy-green with 56 selected rows. In the
current local php-src oracle build, reference rows skip because the zlib
extension is not loaded; the target runtime reports 56 PASS.

```text
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 \
nix develop -c just phpt-dev-module MODULE=zlib
```

A temporary full target-only originals sweep was also run from a generated
manifest outside the repository:

```text
target/debug/php-phpt-tools run \
  --manifest /private/tmp/phrust-zlib-manifest-after-rebase/zlib-originals.jsonl \
  --out /private/tmp/phrust-phpt-zlib-originals-after-rebase/results.jsonl
```

That sweep reported 143 upstream zlib-related originals: PASS 52 / SKIP 20 /
FAIL 71. Thirty-three newly target-green rows not already selected were
promoted. `ext/zlib/tests/bug_34821.phpt` is target-correct with a longer
timeout, but its 50K `rand()`/string-append compression loop is too slow for the
20s selected-gate contract and remains unselected until that performance path
is improved.

## Remaining Gaps

- User-defined stream filters, zlib filter params/window-size parity, and
  incremental multi-chunk stream-filter state parity remain unpromoted.
- `ob_gzhandler`, full SAPI output compression, and related INI interactions
  remain outside the selected gate except for the no-op/header rows listed
  above.
- Full gzip metadata/header parity and all window-size edge cases remain
  unpromoted.
- Large compression loops and byte-by-byte `inflate_add` loops remain outside
  the selected gate until the relevant runtime performance paths are optimized.
- `compress.zlib://` wrapper parity, strict invalid level/max-length warning
  text, and wrapper stat/unlink/rename behavior remain open.
- Process-control dependent upstream gzip stream rows are not selected for the
  php-cli target contract. The generated `gzpassthru` helper is selected and
  target-green; the PHPT tooling process-control skip classifier treats
  `gzpassthru()` separately from the process-control `passthru()` builtin.
