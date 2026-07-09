# ffi PHPT coverage

## Verified scope

- `ffi` extension visibility.
- Generated internal class surface for `FFI`, `FFI\CData`, `FFI\CType`,
  `FFI\Exception`, and `FFI\ParserException`.
- Reflection metadata for the `FFI` class, extension name, internal status, and
  generated static method signatures.
- Fail-closed VM dispatch for unsafe static calls such as `FFI::cdef()`.
- Default-off `ffi.enable` and `ffi.preload` INI visibility.
- Extension-scoped `ini_get_all('ffi')` metadata.
- Read-only runtime mutation policy for unsafe FFI enablement.

## Known gaps

- No libffi, dlopen, symbol lookup, or C ABI execution backend is implemented.
- `CType` and `CData` parsing, allocation, casting, pointer, array, and struct
  semantics are not implemented.
- FFI memory operations are surfaced as metadata only and do not operate on C
  memory.
- Preload execution and named FFI scopes are not implemented.
- Platform ABI constants and exact FFI exception object parity remain future
  work.
- Server-mode policy enforcement is limited to read-only default-off INI state
  and fail-closed runtime dispatch.
