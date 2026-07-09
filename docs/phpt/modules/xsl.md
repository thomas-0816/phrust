# xsl PHPT coverage

## Verified scope

- `xsl` extension visibility.
- `XSLTProcessor` class visibility and reflection metadata as an internal class
  owned by the `xsl` extension.
- Generated `XSLTProcessor` method metadata, including
  `hasExsltSupport()`.
- Stable XSL clone constants: `XSL_CLONE_AUTO`, `XSL_CLONE_NEVER`, and
  `XSL_CLONE_ALWAYS`.
- Stable XSL security-preference constants, including file, directory, network,
  and default masks.
- Deterministic backend gate for `XSLTProcessor` construction when no libxslt
  capability is enabled.

## Known gaps

- No libxslt or libexslt backend is connected.
- `XSLTProcessor` construction and instance methods fail closed instead of
  running stylesheet transforms.
- DOMDocument handle interop for stylesheet import and transform output is not
  implemented.
- PHP callback registration, profiling, transform-to-URI, and security
  preference enforcement remain future work.
- libxslt/libexslt version constants are not surfaced by the selected bounded
  facade.
