# WordPress Bring-up Pack B: Autoload and Stdlib

Pack B owns generic PHP autoload, class-like lookup, feature-detection, callable,
and builtin heatmap closure work for framework bring-up. The implementation must
not special-case WordPress paths, symbols, or files.

## Gate

Run the Pack B fixture slice with:

```bash
nix develop -c env REFERENCE_PHP=/path/to/php-8.5.7/sapi/cli/php just wp-autoload-stdlib
```

The target runs `scripts/runtime_semantics_diff.py --category wp_autoload_stdlib`
and then builds a builtin heatmap with `scripts/wordpress_builtin_heatmap.py`.
Reports are written under:

- `target/runtime-semantics/wp-autoload-stdlib/runtime-semantics-diff-report.json`
- `target/wordpress-bringup/builtin-heatmap.json`
- `target/wordpress-bringup/builtin-heatmap.md`

## Covered Behavior

- SPL autoload stack behavior: register, unregister, stack ordering,
  `spl_autoload_functions()`, recursive lookup guard, and repeated negative
  lookups when callbacks may have side effects.
- Class-like lookup: `class_exists`, `interface_exists`, `trait_exists`, and
  `enum_exists` share the VM lookup/cache path and respect the optional autoload
  argument.
- Runtime class-table validation: classes loaded through autoload re-check
  parent and interface declarations through the live class table after include
  and eval side effects.
- Composer-style classmap autoload: fixture-backed static map includes source
  files and leaves the declared class visible to subsequent non-autoload lookup.
- Feature detection: `function_exists`, class/interface/trait/enum probes,
  `method_exists`, `property_exists`, `defined`, `constant`,
  `extension_loaded`, `get_loaded_extensions`, `get_defined_functions`,
  `get_declared_*`, and `get_defined_constants`.
- Callable helpers: `is_callable`, `call_user_func`, and
  `call_user_func_array` over string functions, static methods, instance
  methods, closures, and invokable objects.
- Core builtin smoke: array/string/config helpers that commonly appear in
  bootstrap code.
- Reflection autoload: `ReflectionClass` triggers the same class-like resolver
  before metadata inspection.

## Heatmap

The heatmap consumes runtime-diff JSON and groups missing or wrong builtin,
class, constant, extension, callable, arity, type, return, and warning/error
observations. Each row records the observed name, owner hint, first source
location, first stack frame when present, count, diagnostic ID, recommended
fixture path, and PHPT coverage hint.

An empty heatmap is valid for a green reduced fixture slice; broader WordPress
or Composer source-mode runs can pass their runtime-diff report with:

```bash
scripts/wordpress_builtin_heatmap.py --input target/runtime-semantics/wordpress-real/runtime-semantics-diff-report.json --out target/wordpress-bringup
```

## Merge Notes

- Pack A can consume the resolver behavior after lowering produces class-like
  names and callable operands; Pack B does not require parser or lowering
  special cases.
- Pack C can use the heatmap output to prioritize web/SAPI, HTTP, MySQLi, and
  request/filesystem builtins without changing the autoload resolver contract.
- Generated heatmap reports stay under `target/` and must not be committed.
