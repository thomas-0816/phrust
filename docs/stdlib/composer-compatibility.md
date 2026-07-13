# Standard library Composer Compatibility

Standard library targets offline Composer compatibility for PHP 8.5.7 (`php-8.5.7`).
The required workflow is:

```bash
nix develop -c just composer-smoke
```

## Required Path

- local PSR-4 fixtures: `tests/fixtures/stdlib/composer/project`
- generated classmap fixtures
- generated files autoload fixtures
- Composer-style `platform_check.php` fixtures
- source-mode smoke without network, plugins, scripts, or Packagist

## Coverage

The standard library wires the CLI include roots into runtime filesystem
capabilities, so `include_path`, `stream_resolve_include_path`,
`include`/`require`, and SPL autoload callbacks see the same bounded local
fixture roots.

The checked-in Composer-style project uses `vendor/autoload.php` to register a
PSR-4-like autoload function. The fixture loads
`Stdlib\ComposerProject\App\Greeter` from
`tests/fixtures/stdlib/composer/project/src`, verifies that a missing class
does not crash, constructs the autoloaded class, and calls an instance method
defined in the included unit.

Validation:

```bash
nix develop -c scripts/stdlib_diff.py --file tests/fixtures/stdlib/_harness/composer/composer_project_autoload.php --out target/stdlib/diff-composer-project-autoload
nix develop -c just composer-smoke
```

## Generated Fixture

`tests/fixtures/stdlib/composer/basic_project` is a checked-in, offline
Composer-like project. It includes:

- `vendor/autoload.php`
- `vendor/composer/autoload_psr4.php`
- `vendor/composer/autoload_classmap.php`
- `vendor/composer/autoload_files.php`
- one PSR-4 class, one classmap class, and one files-autoload helper function

The fixture can be regenerated deterministically without network access:

```bash
nix develop -c just composer-fixture-prepare
```

The differential fixture
`tests/fixtures/stdlib/_harness/composer/basic_project_autoload.php` verifies
that `require 'autoload.php'` loads the files helper, autoloads the PSR-4
class, autoloads the classmap class, and handles a missing class without a
runtime crash.

## Autoload Smoke

`just composer-smoke-autoload` runs the dedicated autoload-order fixture through
the Standard library differential harness:

```bash
nix develop -c just composer-smoke-autoload
```

The smoke writes reference/Rust snapshot details under
`target/stdlib/composer-smoke-autoload`. It verifies files autoload happens
before class method use, PSR-4 and classmap classes load, repeated
`include_once 'autoload.php'` is stable, the autoload stack is not duplicated by
the repeated include, and missing classes remain non-fatal.

## Platform Checks

`just composer-smoke-platform` runs Composer-style platform checks through the
Standard library differential harness:

```bash
nix develop -c just composer-smoke-platform
```

The fixture `vendor/composer/platform_check.php` in the offline
`basic_project` verifies `PHP_VERSION_ID`, `PHP_VERSION`, `defined`,
`constant`, `extension_loaded`, `get_loaded_extensions`, `ini_get`,
`class_exists`, `function_exists`, and `version_compare`. It does not assert the
`mbstring` loaded state because the default PHP 8.5.7 oracle is built without
mbstring while phrust intentionally exposes a bounded UTF-8 mbstring MVP; the
focused mbstring PHPT/module gates own that policy. The companion
`platform_version_compare.php` fixture pins Composer-relevant comparison
operators and prerelease labels against the PHP 8.5.7 reference.

## Process Capability Surface

`just process-capability-smoke` runs a VM-only fixture that proves Composer-facing
process probes are defined but default-off:

```bash
nix develop -c just process-capability-smoke
```

The fixture checks `proc_open`, `popen`, `shell_exec`, `exec`, `passthru`, and
`system` return deterministic failure values and emit controlled
`E_PHP_VM_PROCESS_CAPABILITY_DISABLED` diagnostics instead of launching a shell
or crashing. Differential comparison is intentionally not used for this fixture
because reference PHP would execute the host command.

## Source-Mode Smoke

`just composer-smoke-source` runs an opt-in Composer source checkout smoke:

```bash
nix develop -c just composer-smoke-source
PHRUST_STDLIB_COMPOSER_SOURCE_DIR=/path/to/composer nix develop -c just composer-smoke-source
```

When `PHRUST_STDLIB_COMPOSER_SOURCE_DIR` is unset or missing, the target writes
`target/stdlib/composer-source-smoke/report.json` with `status=skip` and exits
successfully. When it is set, the script rejects `composer.phar`, builds the
local `php-vm` binary, and runs the configured source entry
(`$PHRUST_STDLIB_COMPOSER_SOURCE_ENTRY` or `bin/composer`) with `--no-plugins
--version`. `COMPOSER_HOME` and `COMPOSER_CACHE_DIR` point at
`target/stdlib/composer-source-smoke`, and no Packagist/network setup is
performed.

Failures write `stdout.txt`, `stderr.txt`, `report.json`, and
`missing-symbols.txt`. The missing-symbol list is sorted by frequency and
extracts undefined functions, classes, methods, constants, Reflection method
markers, and SPL method markers from VM diagnostics. `report.json` also records
diagnostic and warning/fatal frequencies separately from missing symbols so the
next compatibility gap is visible without requiring Composer PHAR support.

## Explicit Boundaries

- Composer source mode is required before `composer.phar`.
- PHAR is optional and governed by ADR 0013. Standard library does not implement PHAR
  archive parsing, `phar://`, or stub execution; an optional read-only MVP must
  be accepted separately before any implementation.
- Online Packagist is not a required Standard library gate.
- Composer source mode is opt-in through `PHRUST_STDLIB_COMPOSER_SOURCE_DIR`; the
  repository does not vendor Composer source.
- Process and shell functions are disabled by default. Standard library includes an
  isolated mock for `shell_exec`, `exec`, `system`, and `passthru`; real process
  execution and process resources remain tracked in
  `STDLIB-GAP-PROCESS-CAPABILITY`.
- Host filesystem access is restricted to deterministic fixture and temporary
  directories.

## Reference

Reference comparison uses pinned PHP 8.5.7 via `REFERENCE_PHP` or
`third_party/php-src/sapi/cli/php`; Standard library commands must not silently select a
global system PHP.
