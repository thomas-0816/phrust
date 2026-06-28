# Runtime Builtin Modules

The runtime builtin layer is organized by runtime responsibility. Public
consumers continue to use `php_runtime::builtins`;
the internal layout separates request context, errors, signatures, registry
assembly, and PHP module ownership.

## Layout

- `crates/php_runtime/src/builtins/mod.rs` exports the stable builtin API.
- `context.rs` owns request-local runtime services passed to builtins, including
  output, cwd, include path, filesystem policy, resources, PCRE state, JSON
  state, and emitted diagnostics.
- `error.rs` owns `BuiltinError` and stable diagnostic IDs.
- `signatures.rs` owns the internal function pointer and result aliases.
- `registry.rs` owns `BuiltinEntry`, compatibility classification, and
  deterministic registry lookup.
- `modules/*.rs` own module-level builtin registration slices and the
  implementations for builtins in that module. Cross-module helpers may remain
  in `core.rs` only when they are shared by several module files.

`BuiltinRegistry` flattens the module slices through a `OnceLock`, sorts the
entries by builtin name, and exposes the same stable `entries`, `get`, and
`contains` behavior as before. Sorting at the registry boundary keeps lookup and
test behavior deterministic while allowing module files to group entries by
functional ownership.

## Module Ownership

| Builtin area | Module file |
| --- | --- |
| Registry glue, scalar/type helpers, output/config/env/process placeholders, tokenizer, serialization, var dumping | `builtins/modules/core.rs` |
| Array functions, array callback placeholders, array sorting placeholders | `builtins/modules/arrays.rs` |
| String, formatting, encoding, hashing, URL/HTML, version comparison | `builtins/modules/strings.rs` |
| Numeric and math functions | `builtins/modules/math.rs` |
| Path and filesystem functions | `builtins/modules/filesystem.rs` |
| Resource streams, directories, stream metadata/context/include-path helpers | `builtins/modules/streams.rs` |
| JSON encode/decode/validate and JSON last-error functions | `builtins/modules/json.rs` |
| PCRE functions and PCRE last-error functions | `builtins/modules/pcre.rs` |
| Date/time/timezone functions | `builtins/modules/date.rs` |
| SPL object helpers and SPL autoload placeholders | `builtins/modules/spl.rs` |
| Symbol introspection, callable dispatch placeholders, class/function/method existence helpers | `builtins/modules/reflection.rs` |

## Prompt 15.1 Ownership Check

Prompt 15.1 verified that filesystem and stream builtins are owned by their
module files rather than routed through temporary `core.rs` shims:

- filesystem/path builtins such as `file_exists`, `file_get_contents`,
  `file_put_contents`, `readfile`, `mkdir`, `rename`, `unlink`, `getcwd`, and
  `chdir` are registered and implemented in
  `crates/php_runtime/src/builtins/modules/filesystem.rs`
- resource stream and stream helper builtins such as `fopen`, `fread`,
  `fwrite`, `fclose`, `stream_get_contents`, `stream_get_meta_data`, and
  `stream_resolve_include_path` are registered and implemented in
  `crates/php_runtime/src/builtins/modules/streams.rs`
- `core.rs` retains only shared helper functions and cross-module tests for
  this area

Prompt 15.1 validation:

- `nix develop -c cargo test -p php_runtime`: PASS
- `nix develop -c just diff-streams`: PASS, 2 pass / 0 fail / 0 skip /
  0 known-gap
- `nix develop -c just verify-stdlib`: PASS

## Adding Builtins

New standard-library functions should be added to the file matching their PHP
module ownership, and their `BuiltinEntry` should be added to that file's
`ENTRIES` slice. Avoid registry-only shims that point back into `core.rs`.
Shared helpers belong in `core.rs` only when they are reused across module
boundaries; otherwise keep helpers private to the module that owns the builtin.

Do not add files or registries whose ownership is based on implementation
history. Unsupported behavior should remain explicit through stable runtime
diagnostics or VM-level placeholders rather than silently returning plausible
values.
