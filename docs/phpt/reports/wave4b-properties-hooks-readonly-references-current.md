# Wave 4B Properties, Hooks, Readonly, and References Current

Date: 2026-06-30

## Focus

This branch narrows the runtime property-reference and clone-with known gaps for
ordinary object-property storage. The rebased base also keeps the full selected
`objects.classes` gate green.

## PHPT Impact

Command:

```bash
REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php \
PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src \
PHPT_REUSE_LAST=0 \
PHPT_DEV_REUSE_TARGET_PASS=0 \
nix develop -c just phpt-dev-module MODULE=objects.classes
```

Result:

- reference: 246 PASS, 0 non-green
- target: 246 PASS, 0 FAIL

No target failures remain in the selected `objects.classes` set after rebasing
onto `origin/main`.

## Focused Runtime Results

- `fixtures/runtime_semantics/properties`: 6 PASS / 0 FAIL
- `fixtures/runtime_semantics/property_hooks`: 6 PASS / 2 known gaps / 0 FAIL
- `fixtures/runtime_semantics/clone_with`: 6 PASS / 4 known gaps / 0 FAIL
- `fixtures/runtime_semantics/refs`: 9 PASS / 0 FAIL
- `fixtures/runtime_semantics/types`: 11 PASS / 5 known gaps / 0 FAIL
