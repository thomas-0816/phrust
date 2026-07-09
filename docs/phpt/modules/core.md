# core PHPT coverage

## Verified scope

- Pinned PHP 8.5.7 version constants.
- Core error constants used by framework bootstrap checks.
- `extension_loaded()`, `php_sapi_name()`, and INI function visibility.
- Selected `ini_get()`, `ini_set()`, and `ini_get_all()` behavior.
- Core `get_defined_constants()` and `get_defined_functions()` visibility.
- Core throwable, exception, and error class/interface visibility.

## Known gaps

- This selected manifest is a core bootstrap slice, not full Zend engine parity.
- Exact diagnostics for every core function and every engine error remain
  covered only where selected by narrower fixtures.
- Additional INI directive ownership, upload, memory, and timeout behavior
  should be promoted as explicit fixtures before being claimed.
