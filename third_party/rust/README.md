# Vendored Rust Crates

This directory contains small patched Rust crates used through the workspace
`[patch.crates-io]` section.

- `pcre2` provides runtime APIs that are not available from the published
  crates.io release used by the workspace.
- `tiger` provides the Tiger4 hash surface needed by the PHP hash extension.

Keep these directories committed whenever `Cargo.toml` points patches here so
fresh checkouts can build without relying on local Cargo cache state.
