---
name: new-phpt-fixture
description: Scaffold a minimized generated PHPT regression fixture (with provenance) under tests/phpt/generated/<module>/, as required for every behavior fix in phrust. Use when a runtime or frontend fix needs a focused, committed regression test (e.g. /new-phpt-fixture).
disable-model-invocation: true
---

# Create a minimized generated PHPT fixture

Every behavior fix in `phrust` needs a focused regression fixture. For new
behavior that isn't covered by a runnable upstream PHPT, add a **minimized
generated PHPT with provenance** here:

```
tests/phpt/generated/<module>/<short-kebab-name>.phpt
```

`<module>` is the functional module the behavior belongs to (e.g. `zend.basic`,
`operators.conversions`, `standard.strings`). Never copy or move a file out of
`third_party/php-src/` — write a new minimized case.

## Template (match the repo's existing convention)

```
--TEST--
Generated <module>: <one-line behavior being pinned>
--DESCRIPTION--
module: <module>
generated timestamp: <YYYYMMDDThhmmssZ>
generator version: phpt-<module>-v1
reason: <why this fixture exists / the exact behavior it locks in>
--FILE--
<?php
// smallest program that reproduces the behavior
?>
--EXPECT--
<exact reference output>
```

Use `--EXPECTF--` (placeholders like `%s`, `%d`, `%a`) or `--EXPECTREGEX--` only
when the output is genuinely nondeterministic; prefer exact `--EXPECT--`.

## Steps

1. Pick the smallest PHP snippet that reproduces the fixed behavior.
2. Get the authoritative expected output from the PHP **8.5.7** oracle:
   `nix develop -c third_party/php-src/sapi/cli/php /path/to/snippet.php`
   Paste that verbatim into `--EXPECT--`.
3. Write the `.phpt` under `tests/phpt/generated/<module>/` using the template;
   fill `reason` with the concrete behavior (not "bug fix").
4. Verify it passes against the engine with the narrowest run:
   `nix develop -c just phpt-dev-build`
   `nix develop -c just phpt-fast MODULE=<module> FILE=tests/phpt/generated/<module>/<name>.phpt`
5. Re-project/triage so the new fixture is registered, then run the module +
   `verify-phpt` gate before handoff. Do not hand-edit baseline manifests —
   regenerate via `just phpt-triage`.

## Don't

- Don't edit `third_party/php-src/` or commit `target/` artifacts.
- Don't invent expected output — it must come from the 8.5.7 oracle.
- Don't add a large upstream test verbatim; minimize it and record provenance.
