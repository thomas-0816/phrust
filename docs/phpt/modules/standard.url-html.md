# standard.url-html PHPT coverage

## Verified scope

- URL encoding helpers.
- `http_build_query()` arrays, objects with visible properties, references,
  nulls, output-separator defaults, resources, recursion suppression, and
  RFC3986 encoding.
- Basic `parse_url()` component extraction and PHP URL constant ordering.
- `parse_str()` basics, custom `arg_separator.input`, malformed key recovery,
  and invalid percent preservation.
- HTML escaping and entity helpers selected by the module manifest.
- `getenv()` and `putenv()` request-environment mutation semantics.
- PHP-visible behavior is verified through 54 selected PHPT fixtures against
  the php-src oracle.

## Known gaps

- Full URL parser, query-string, and HTML entity table parity is not claimed.
- Encoding-specific entity behavior and all malformed-input diagnostics remain
  limited to selected fixtures.
