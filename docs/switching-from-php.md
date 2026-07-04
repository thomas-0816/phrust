# Switching From PHP

`phrust-php` is the entrypoint to try first when replacing a local `php`
command in compatibility experiments.

## Local Shell Shim

```bash
nix develop -c just install-user-bin
export PATH="$PWD/target/phrust/bin:$PATH"
php -v
```

The shim creates `target/phrust/bin/php` as a symlink to `phrust-php`. It is
local to the checkout and only takes effect in shells where that directory is
first in `PATH`.

## Command Mapping

| PHP command | Phrust command | Status |
| --- | --- | --- |
| `php -v` | `phrust-php -v` | implemented |
| `php -r 'code'` | `phrust-php -r 'code'` | implemented |
| `php script.php arg` | `phrust-php script.php arg` | implemented |
| `php -l script.php` | `phrust-php -l script.php` | compile-only lint |
| `php -m` | `phrust-php -m` | implemented from the registered stdlib surface |
| `php -i` | `phrust-php -i` | minimal phpinfo-style output |
| `php --ini` | `phrust-php --ini` | implemented for explicit `-c` and `-n` |
| `php -S 127.0.0.1:8080 -t public` | `phrust-php -S 127.0.0.1:8080 -t public` | implemented |

## Limitations

Phrust does not provide Zend extension ABI loading, FPM, FastCGI, CGI, Apache
module integration, Opcache, phpdbg, or a production SAPI. Reflection-style CLI
introspection flags `--ri`, `--rf`, and `--rc` are recognized but currently
return a stable unsupported diagnostic.

See [PHP user interface matrix](php-user-interface-matrix.md) for the current
surface summary and [Compatibility](compatibility.md) for broader engine gaps.
