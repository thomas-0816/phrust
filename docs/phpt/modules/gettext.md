# gettext PHPT coverage

Current focused coverage:

- `extension_loaded("gettext")` and function visibility for gettext aliases and
  domain helpers.
- Untranslated fallback behavior for `gettext()`, `_()`, `dgettext()`, and
  `dcgettext()`.
- Singular/plural fallback selection for `ngettext()`, `dngettext()`, and
  `dcngettext()`.
- Request-local `textdomain()`, `bindtextdomain()`, and
  `bind_textdomain_codeset()` state.
- PHP-compatible `LC_ALL` category rejection for category-specific lookups.
- Deterministic GNU MO catalog parsing from generated fixtures.
- Request-local `LC_ALL`, category-specific `LC_MESSAGES`/`LC_CTYPE`, and
  `LANG` lookup through VM-controlled `putenv()` state.
- LC_MESSAGES and LC_CTYPE catalog directories under bound domain paths.
- Bounded plural metadata handling for common `nplurals=1`, `n > 1`, and
  `n != 1` forms.
- Upstream diagnostics for missing bind paths, empty domains, codeset
  return-values, `dcngettext()`, LC_ALL category rejection, and overlong
  arguments.
- Null-byte domain diagnostics for `bindtextdomain()` through the upstream
  `gh17400` regression.

The selected PHPTs are deterministic and do not require host locale data or
checked-in external catalog files. Full gettext parity remains outside this
slice: host-locale `setlocale()` behavior beyond `C`/`POSIX`, complete GNU
plural expression evaluation, codeset conversion, native libintl-backed
behavior, and the remaining upstream tests that depend on installed locale
catalogs and mutable process locale state.

Focused gate:

```bash
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=gettext
```
