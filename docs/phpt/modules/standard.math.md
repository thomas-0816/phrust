# standard.math PHPT coverage

## Verified scope

- Selected math and numeric standard builtins from the php-src oracle.
- The current selected gate verifies 161 PASS and 11 SKIP target outcomes from
  172 selected fixtures.

## Known gaps

- Full numeric and math standard-library parity is not claimed.
- Platform-dependent skips are preserved rather than reclassified.
- Floating-point formatting, warning text, and architecture-dependent edge
  cases remain limited to the selected corpus.
