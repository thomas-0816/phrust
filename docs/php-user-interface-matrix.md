# PHP User Interface Matrix

This matrix tracks the user-facing PHP command surfaces exposed by Phrust.

| Surface | Entry point | Current behavior |
| --- | --- | --- |
| CLI version | `phrust-php -v` | prints the targeted PHP version and Phrust marker |
| Inline code | `phrust-php -r <code>` | executes code without requiring `<?php` |
| Script file | `phrust-php script.php [args...]` | seeds `$argv`, `$argc`, and `$_SERVER` |
| Standard input source | `phrust-php` | reads PHP source from stdin |
| Piped stdin resource | `printf data \| phrust-php -r '...'` | exposes bytes through `STDIN` |
| INI file | `phrust-php -c php.ini` | loads `include_path`, `display_errors`, and `error_reporting` |
| INI override | `phrust-php -d name=value` | overrides values loaded from `-c` |
| INI report | `phrust-php --ini` | reports explicit loaded config or none |
| Lint | `phrust-php -l script.php` | compiles only and does not execute side effects |
| Module list | `phrust-php -m` | lists enabled standard-library extension descriptors |
| phpinfo | `phrust-php -i` | prints minimal PHP version, SAPI, binary, INI, and module data |
| Built-in server | `phrust-php -S addr -t public` | starts the in-process HTTP server |
| Built-in server router | `phrust-php -S addr -t public router.php` | router runs before normal static/PHP routing; explicit `false` falls through |
| Local shim | `just install-user-bin` | creates `target/phrust/bin/php -> phrust-php` |
| Developer VM CLI | `php-vm` | retained for VM debugging, not the PHP-user front door |

## Known Limitations

- `--ri`, `--rf`, and `--rc` are recognized and fail with an explicit
  unsupported diagnostic.
- The `-c` parser is intentionally minimal and deterministic; it is not a full
  php.ini parser.
- The built-in server does not implement FPM, FastCGI, CGI, Apache module
  integration, Zend extension ABI loading, Opcache, or phpdbg.

## Checks

```bash
nix develop -c just cli-interface-smoke
nix develop -c just cli-server-smoke
nix develop -c just verify-user-interfaces
```
