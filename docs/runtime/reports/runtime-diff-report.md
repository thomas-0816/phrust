# Runtime Compatibility Report

- Fixtures: 153
- Pass: 112
- Unexpected failures: 0
- Skipped: 3
- Expected known gaps: 9
- Unexpected passes: 29

## Categories

- `DiagnosticMismatch`: 1
- `ExpectedKnownGap`: 6
- `RuntimeExitMismatch`: 1
- `UnexpectedPass`: 29
- `UnsupportedFeature`: 1

## Feature Areas

- `Array spread/unpack in literals`: 1
- `By-reference foreach over temporary or nonlocal sources`: 1
- `Complete `$GLOBALS` alias table semantics`: 1
- `Complete `var_dump` formatting matrix`: 4
- `Complete enum runtime semantics`: 1
- `Complete finally interaction with catch-thrown control-flow, break/continue, destructors, generators, fibers, and nested handlers`: 1
- `Complete include scope and symbol side effects`: 1
- `Complete property-hook runtime semantics`: 1
- `Complete superglobal and SAPI request matrix`: 1
- `Complete weak/strict type coercion matrix`: 1
- `Complex mutation during array iteration`: 2
- `Full PHP array semantics for variadic parameters`: 1
- `Full PHP builtin type coercion and diagnostic matrix`: 1
- `Full PHP include/require warning text and include_path search`: 2
- `Full PHP numeric-string conversion and comparison matrix`: 1
- `Full PHP standard library and extensions`: 1
- `Full PHP undefined-variable warning wording and variable names`: 1
- `Full PHP warning output channel and wording compatibility`: 3
- `Full array/reference Copy-on-Write matrix`: 1
- `Full references and Copy-on-Write`: 4
- `Full runtime evaluation of constant expressions`: 1
- `Includes outside configured roots`: 1
- `Property modifier edges outside the covered object MVP`: 2
- `Reflection outside the Work item metadata MVP`: 1
- `Remaining clone magic and clone-with edge cases`: 1
- `Remaining trait composition gaps`: 1
- `arrays`: 5
- `autoload`: 1
- `builtins`: 3
- `constants`: 6
- `control_flow`: 14
- `corpus_smoke`: 7
- `division-by-zero.php`: 1
- `errors`: 1
- `eval`: 1
- `exceptions`: 6
- `fibers`: 1
- `foreach`: 4
- `functions`: 16
- `generators`: 2
- `hello.php`: 1
- `includes`: 5
- `match-no-arm.php`: 1
- `objects`: 15
- `php85`: 5
- `references`: 1
- `runtime-error.php`: 1
- `runtime_types`: 8
- `scalars`: 3
- `superglobals`: 3
- `type-error.php`: 1
- `variables`: 4

## Diagnostic IDs

- `E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH`: 1
- `E_PHP_RETURN_VALUE_FROM_VOID_FUNCTION`: 1
- `E_PHP_RUNTIME_DIVISION_BY_ZERO`: 2
- `E_PHP_RUNTIME_NON_NUMERIC_STRING`: 1
- `E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING`: 1
- `E_PHP_RUNTIME_UNDEFINED_CONSTANT`: 1
- `E_PHP_RUNTIME_UNDEFINED_FUNCTION`: 4
- `E_PHP_STD_TYPE_ERROR`: 1
- `E_PHP_VM_INCLUDE_MISSING`: 3
- `E_PHP_VM_RETURN_TYPE_MISMATCH`: 2
- `E_PHP_VM_UNCAUGHT_EXCEPTION`: 12
- `E_PHP_VM_UNKNOWN_CLASS`: 2

## Owner Streams

- `runtime-semantics`: 37

## Non-Pass Fixtures

| Fixture | Status | Category | Known gap | Feature area | Owner | First differing line | Message |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `fixtures/runtime/governance/diagnostic-mismatch.php` | `KnownGap` | `DiagnosticMismatch` | `E_PHP_RUNTIME_BUILTIN_TYPE` | Full PHP builtin type coercion and diagnostic matrix | runtime-semantics | 1 | - |
| `fixtures/runtime/governance/expected-known-gap.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_WARNING_CHANNEL_COMPAT` | Full PHP warning output channel and wording compatibility | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/governance/runtime-exit-mismatch.php` | `KnownGap` | `RuntimeExitMismatch` | `E_PHP_VM_INCLUDE_MISSING` | Full PHP include/require warning text and include_path search | runtime-semantics | 4 | - |
| `fixtures/runtime/governance/stdout-mismatch.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_VAR_DUMP_FORMAT_MATRIX` | Complete `var_dump` formatting matrix | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/governance/unexpected-pass.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_UNDEFINED_VARIABLE_WARNING` | Full PHP undefined-variable warning wording and variable names | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/governance/unsupported-feature.php` | `KnownGap` | `UnsupportedFeature` | `E_PHP_RUNTIME_UNSUPPORTED_STDLIB` | Full PHP standard library and extensions | runtime-semantics | 1 | - |
| `fixtures/runtime/known_gaps/autoload/spl-autoload-register.php` | `KnownGap` | `ExpectedKnownGap` | - | autoload | - | 2 | - |
| `fixtures/runtime/known_gaps/foreach/by-ref.php` | `KnownGap` | `ExpectedKnownGap` | `E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH` | By-reference foreach over temporary or nonlocal sources | runtime-semantics | 1 | - |
| `fixtures/runtime/known_gaps/objects/clone-with-private.php` | `KnownGap` | `ExpectedKnownGap` | `E_PHP_IR_UNSUPPORTED_OBJECT_PROPERTY_MODIFIER` | Property modifier edges outside the covered object MVP | runtime-semantics | 4 | - |
| `fixtures/runtime/known_gaps/objects/clone-with-readonly.php` | `KnownGap` | `ExpectedKnownGap` | `E_PHP_IR_UNSUPPORTED_OBJECT_PROPERTY_MODIFIER` | Property modifier edges outside the covered object MVP | runtime-semantics | 2 | - |
| `fixtures/runtime/valid/arrays/missing-key.php` | `KnownGap` | `ExpectedKnownGap` | `E_PHP_RUNTIME_WARNING_CHANNEL_COMPAT` | Full PHP warning output channel and wording compatibility | runtime-semantics | 1 | - |
| `fixtures/runtime/valid/arrays/spread-list-string-keys.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_IR_UNSUPPORTED_ARRAY_SPREAD` | Array spread/unpack in literals | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/arrays/var-dump-mixed.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_VAR_DUMP_FORMAT_MATRIX` | Complete `var_dump` formatting matrix | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/builtins/var-dump-array.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_VAR_DUMP_FORMAT_MATRIX` | Complete `var_dump` formatting matrix | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/builtins/var-dump-scalars.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_VAR_DUMP_FORMAT_MATRIX` | Complete `var_dump` formatting matrix | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/constants/global.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_CONST_EXPR_MATRIX` | Full runtime evaluation of constant expressions | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/enums/unit-enum.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_IR_UNSUPPORTED_ENUM_RUNTIME` | Complete enum runtime semantics | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/errors/warning-continuation.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_WARNING_CHANNEL_COMPAT` | Full PHP warning output channel and wording compatibility | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/exceptions/finally-return.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_UNSUPPORTED_FINALLY_EDGE_MATRIX` | Complete finally interaction with catch-thrown control-flow, break/continue, destructors, generators, fibers, and nested handlers | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/foreach/by-ref-break-continue.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_FOREACH_MUTATION_COMPAT` | Complex mutation during array iteration | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/foreach/snapshot-mutation.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_FOREACH_MUTATION_COMPAT` | Complex mutation during array iteration | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/functions/by-ref-capture.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_UNSUPPORTED_REFERENCE_SEMANTICS` | Full references and Copy-on-Write | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/functions/variadic-sum.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_VARIADIC_PACKED_ARRAY_ONLY` | Full PHP array semantics for variadic parameters | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/includes/include-missing.php` | `KnownGap` | `ExpectedKnownGap` | `E_PHP_VM_INCLUDE_MISSING` | Full PHP include/require warning text and include_path search | runtime-semantics | 4 | stdout reference="before\|\nWarning: include(/Volumes/CrucialMusic/src/phrust_branches/phrust4/fixtures/runtime/valid/includes/lib/missing.php): Failed to open stream: No such file or directory in /Volumes/CrucialMusic/src/phrust_branches/phrust4/fixtures/runtime/valid/includes/include-missing.php on line 4\n\nWarning: include(): Failed opening '/Volumes/CrucialMusic/src/phrust_branches/phrust4/fixtures/runtime/valid/includes/lib/missing.php' for inclusion (include_path='.:') in /Volumes/CrucialMusic/src/phrust_branches/phrust4/fixtures/runtime/valid/includes/include-missing.php on line 4\nafter\n" rust="before\|\nWarning: include(/Volumes/CrucialMusic/src/phrust_branches/phrust4/fixtures/runtime/valid/includes/lib/missing.php): Failed to open stream: No such file or directory in /Volumes/CrucialMusic/src/phrust_branches/phrust4/fixtures/runtime/valid/includes/include-missing.php on line 4\n\nWarning: include(): Failed opening '/Volumes/CrucialMusic/src/phrust_branches/phrust4/fixtures/runtime/valid/includes/lib/missing.php' for inclusion (include_path='.') in /Volumes/CrucialMusic/src/phrust_branches/phrust4/fixtures/runtime/valid/includes/include-missing.php on line 4\nafter\n"; stderr reference="" rust="{file}: runtime-diagnostic: {\"id\":\"E_PHP_VM_INCLUDE_MISSING\",\"severity\":\"warning\",\"message\":\"E_PHP_VM_INCLUDE_MISSING: /Volumes/CrucialMusic/src/phrust_branches/phrust4/fixtures/runtime/valid/includes/lib/missing.php: No such file or directory (os error 2)\",\"span\":{\"file\":\"/Volumes/CrucialMusic/src/phrust_branches/phrust4/{file}\",\"start\":92,\"end\":130},\"stack\":[{\"function\":\"main\"}],\"php_reference\":null}\n" |
| `fixtures/runtime/valid/includes/include-return.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_VM_INCLUDE_OUTSIDE_ROOT` | Includes outside configured roots | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/includes/lib/once.php` | `Skipped` | - | - | includes | - | - | fixture metadata requested skip |
| `fixtures/runtime/valid/includes/lib/return-value.php` | `Skipped` | - | - | includes | - | - | fixture metadata requested skip |
| `fixtures/runtime/valid/includes/lib/share-variable.php` | `Skipped` | - | - | includes | - | - | fixture metadata requested skip |
| `fixtures/runtime/valid/includes/share-variable.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_INCLUDE_SCOPE_MATRIX` | Complete include scope and symbol side effects | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/objects/clone-object.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_UNSUPPORTED_CLONE_MAGIC` | Remaining clone magic and clone-with edge cases | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/property_hooks/get-hook.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_IR_UNSUPPORTED_PROPERTY_HOOKS` | Complete property-hook runtime semantics | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/references/array-element-ref.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_ARRAY_REFERENCE_COW` | Full array/reference Copy-on-Write matrix | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/references/by-ref-param.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_UNSUPPORTED_REFERENCE_SEMANTICS` | Full references and Copy-on-Write | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/references/by-ref-return.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_UNSUPPORTED_REFERENCE_SEMANTICS` | Full references and Copy-on-Write | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/references/local-alias.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_UNSUPPORTED_REFERENCE_SEMANTICS` | Full references and Copy-on-Write | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/reflection/reflection-class.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_IR_UNSUPPORTED_REFLECTION` | Reflection outside the Work item metadata MVP | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/runtime_types/param-int.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_WEAK_STRICT_TYPES_COERCION` | Complete weak/strict type coercion matrix | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/scalars/expressions.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_NUMERIC_STRING_MATRIX` | Full PHP numeric-string conversion and comparison matrix | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/superglobals/empty-superglobals.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_SUPERGLOBALS_FULL_MATRIX` | Complete superglobal and SAPI request matrix | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/superglobals/globals-alias.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_RUNTIME_GLOBALS_ALIAS_MATRIX` | Complete `$GLOBALS` alias table semantics | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
| `fixtures/runtime/valid/traits/trait-use.php` | `UnexpectedPass` | `UnexpectedPass` | `E_PHP_IR_UNSUPPORTED_TRAIT_RUNTIME` | Remaining trait composition gaps | runtime-semantics | - | known-gap fixture now matches the PHP reference; retire or reclassify the gap |
