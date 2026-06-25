# filesystem.streams

- Priority: 11
- Selected manifest: `tests/phpt/manifests/modules/filesystem.streams.selected.jsonl`
- Current counts: 66 PASS, 217 SKIP, 849 FAIL, 0 BORK from 1194 corpus candidates

## Scope

- local filesystem
- streams
- resources
- include_path
- include/require

## Non-Scope

- network streams
- PHAR streams

## Relevant PHPT Paths

- `tests/security/open_basedir_file_put_contents.phpt`
- `tests/security/open_basedir_file_get_contents.phpt`
- `tests/security/open_basedir_file_exists.phpt`
- `tests/output/stream_isatty_non_castable_userwrapper.phpt`
- `tests/output/stream_isatty_non_castable_user_stream.phpt`
- `tests/output/stream_isatty_no_warning_on_cast.phpt`
- `tests/output/sapi_windows_vt100_support_non_castable_user_stream.phpt`
- `tests/basic/rfc1867_max_file_uploads_empty_files.phpt`
- `tests/basic/rfc1867_max_file_size.phpt`
- `tests/basic/rfc1867_file_upload_disabled.phpt`
- `ext/zlib/tests/zlib_scheme_file_read_file_basic.phpt`
- `ext/zlib/tests/zlib_scheme_file_put_contents_basic.phpt`
- `ext/zlib/tests/zlib_scheme_file_get_contents_basic.phpt`
- `ext/zlib/tests/zlib_scheme_file_basic.phpt`
- `ext/zlib/tests/readgzfile_variation7.phpt`
- `ext/zlib/tests/readgzfile_variation15.phpt`
- `ext/zlib/tests/readgzfile_basic2.phpt`
- `ext/zlib/tests/readgzfile_basic.phpt`
- `ext/zlib/tests/gzfile_variation7.phpt`
- `ext/zlib/tests/gzfile_variation15.phpt`
- `ext/zlib/tests/gzfile_open_gz.phpt`
- `ext/zlib/tests/gzfile_basic2.phpt`
- `ext/zlib/tests/gzfile_basic.phpt`
- `ext/zip/tests/stream_meta_data.phpt`
- `ext/zip/tests/oo_stream_seek.phpt`
- `ext/zip/tests/oo_stream.phpt`
- `ext/zip/tests/oo_getstreamindex.phpt`
- `ext/zip/tests/oo_addfile_proc.phpt`
- `ext/zend_test/tests/observer_declarations_file_cache.phpt`
- `ext/xmlwriter/tests/xmlwriter_toStream_open_invalidated_stream.phpt`
- `ext/xmlwriter/tests/xmlwriter_toStream_invalidate_stream.phpt`
- `ext/xmlreader/tests/fromStream_broken_stream.phpt`
- `ext/tidy/tests/parsing_file_too_large.phpt`
- `ext/standard/tests/streams/user_streams_context_001.phpt`
- `ext/standard/tests/streams/user_streams_consumed_bug.phpt`
- `ext/standard/tests/streams/user-stream-open-bailout.phpt`
- `ext/standard/tests/streams/user-stream-dir-open-bailout.phpt`
- `ext/standard/tests/streams/temp_stream_seek.phpt`
- `ext/standard/tests/streams/stream_socket_recvfrom.phpt`
- `ext/standard/tests/streams/stream_socket_pair.phpt`

## Relevant php-src Source Areas

- `ext/standard/tests/file/`
- `ext/standard/tests/streams/`
- `crates/php_runtime/`

## Target Gates

- `nix develop -c just phpt-module MODULE=filesystem.streams`

## Known Gaps

- `runtime-error-or-diagnostic`: 559
- `runtime-unsupported-feature`: 353
- `runtime-output-mismatch`: 184
- `frontend-parse-or-compile`: 4

## Next Step

Keep filesystem policy root-constrained and deterministic.
