# CLI Usage

`phrust-php` is the PHP-compatible command-line front door. It accepts common
PHP CLI flags and executes code through the workspace frontend, runtime, and VM.

## Run PHP Code

```bash
nix develop -c cargo run -p php_vm_cli --bin phrust-php -- path/to/file.php
nix develop -c cargo run -p php_vm_cli --bin phrust-php -- -r 'echo PHP_VERSION, "\n";'
printf '<?php echo "stdin\n";' | nix develop -c cargo run -p php_vm_cli --bin phrust-php --
```

The debug executable built by Cargo is:

```text
target/debug/phrust-php
```

## Common PHP Flags

```bash
phrust-php -v
phrust-php --ini
phrust-php -c php.ini -d display_errors=1 script.php
phrust-php -l script.php
phrust-php -m
phrust-php -i
phrust-php -S 127.0.0.1:8080 -t public
```

The `-c` loader intentionally supports a minimal deterministic subset:
`include_path`, `display_errors`, and `error_reporting`. Blank lines, comments,
and sections are ignored. Repeated `-d` values override values loaded from `-c`.

`--ri`, `--rf`, and `--rc` are recognized and fail with a stable unsupported
diagnostic until reflection-style CLI introspection is implemented.

## Local PHP Shim

```bash
nix develop -c just install-user-bin
export PATH="$PWD/target/phrust/bin:$PATH"
php -v
```

The shim only changes the shell where the exported `PATH` is active. It does
not install into system directories.

## Developer VM CLI

`php-vm` remains available for lower-level VM and bytecode debugging:

```bash
nix develop -c cargo run -p php_vm_cli --bin php-vm -- run path/to/file.php
```

Prefer `phrust-php` for PHP-user compatibility checks and `php-vm` for
developer-only VM inspection.

## Related Docs

- [Switching from PHP](switching-from-php.md)
- [PHP user interface matrix](php-user-interface-matrix.md)
- [Web server](web-server.md)
- [Compatibility](compatibility.md)
- [Contributor guide](contributing.md)
