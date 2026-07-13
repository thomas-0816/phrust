# Standard library INI Config MVP

Reference target: PHP 8.5.7 (`php-8.5.7`).

The standard library provides a deterministic request-local INI registry. The VM seeds it
from `RuntimeContext` at the start of execution, and `ini_set` mutates only that
request-local registry.

Supported PHP-visible functions:

- `ini_get`
- `ini_set`
- `ini_get_all`
- `get_cfg_var`

Documented deterministic entries:

| Option | Default |
| --- | --- |
| `include_path` | `.` |
| `error_reporting` | `-1` |
| `display_errors` | `1` |
| `default_charset` | `UTF-8` |
| `date.timezone` | `UTC` |
| `memory_limit` | `128M` |

`include_path` is consumed by the local include loader. Relative include
targets are searched through the current `include_path` value before falling
back to the including file directory and runtime cwd. Stream wrappers and
remote paths remain outside this work item and continue to fail deterministically.

Unsupported INI entries return `false` from `ini_get`, `ini_set`, and
`get_cfg_var`. `ini_get_all` returns a deterministic array for supported
entries, either in detailed shape or flat local-value shape.
