# Synthetic Plugin/Theme Fixture

This fixture exercises plugin and theme style flows without bundling third-party
code. It is intended for fast local server smoke checks.

Example:

```bash
nix develop -c cargo run -p php_server --bin phrust-server -- \
  --listen 127.0.0.1:8080 \
  --docroot fixtures/integration/plugin_theme_synthetic/public \
  --front-controller index.php
```

Useful requests:

```bash
curl -i 'http://127.0.0.1:8080/?name=demo'
curl -i 'http://127.0.0.1:8080/?redirect=1'
curl -i -F 'package=@fixtures/integration/plugin_theme_synthetic/package/sample.txt' \
  'http://127.0.0.1:8080/?upload=1'
```
