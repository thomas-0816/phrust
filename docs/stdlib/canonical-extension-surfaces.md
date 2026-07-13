# Canonical Extension Surfaces

Extension metadata is owned by `fixtures/stdlib/extensions/`. `index.json`
defines the stable extension and runtime-module order; each extension has one
descriptor containing its functions, classes, constants, lifecycle metadata,
and explicit implementation bindings.

Function signatures and source provenance come from the committed PHP 8.5.7
arginfo snapshot. External functions without pinned stubs carry a reviewed
`signature_gap` in their owning descriptor. An implementation remains
handwritten, but its `BuiltinEntry` must match the descriptor's `runtime` or
`extension` mapping. VM-mediated functions declare an additional `vm` mapping.

Regenerate checked Rust artifacts with:

```bash
nix develop -c just generate-extension-surfaces
```

The generator writes per-extension modules for `php_std`, per-runtime-module
signature indexes for `php_runtime`, and lifecycle metadata for
`php_extensions`. Normal builds perform no network access. CI and local
verification regenerate into `target/` and compare every artifact:

```bash
nix develop -c just verify-generated-extension-surfaces
nix develop -c just stdlib-registry-drift
```

Both gates reject duplicate owners, missing implementation mappings, stale
arginfo provenance, runtime pointer-map drift, unstable ordering, and generated
output drift. New extension symbols must therefore be added to one canonical
descriptor and regenerated; a partial update fails before runtime tests.
