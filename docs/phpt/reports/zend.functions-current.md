# zend.functions Current Focus Report

Generated from:

- `nix develop -c just phpt-dev-build`
- `nix develop -c just phpt-dev-module MODULE=zend.functions`
- `nix develop -c just phpt-rerun-failures MODULE=zend.functions`
- `PHPT_MANIFEST=tests/phpt/manifests/zend.functions-generated.jsonl nix develop -c just phpt-dev-module MODULE=zend.functions`
- `nix develop -c cargo test -p php_std`
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c cargo test -p php_ir`
- `nix develop -c just verify-stdlib`
- `nix develop -c just verify-runtime`

Current focused selected run:

| Outcome | Count |
| --- | ---: |
| PASS | 29 |
| FAIL | 0 |
| SKIP | 0 |
| BORK | 0 |

The selected manifest is the generated Prompt 13 contract set and is green for
both reference and target. A broader 200-row php-src blocker slice was also run:
the reference run was green for all 200 PHPTs, while the target run remained
83 PASS and 117 FAIL. That broader slice is retained below as backlog analysis,
not as the Prompt 13 close gate.

| Broader php-src blocker slice outcome | Count |
| --- | ---: |
| PASS | 83 |
| FAIL | 117 |
| SKIP | 0 |
| BORK | 0 |

## Prompt 13.3 Arginfo Arity Status

Internal registry builtins now run through generated `php_std::arginfo`
metadata for arity and supported scalar coercion before their module-owned
function bodies execute. Builtins with custom validation or by-reference
metadata keep their existing specialized validation path.

Generated PHPT contracts now cover:

- `builtin-too-few-args.phpt`
- `builtin-too-many-args.phpt`
- `variadic-builtin-arity.phpt`
- strict and weak scalar coercion through generated arginfo

Remaining focused `zend.functions` non-green outcomes do not have
`E_PHP_STD_MISSING_ARGUMENT` or `E_PHP_STD_TOO_MANY_ARGUMENTS` as their primary
blocker. The remaining arity-adjacent failures are user-function defaults,
constant-expression defaults, by-reference returns, callable acquisition, and
Closure metadata/output parity rather than builtin arginfo dispatch.

## Prompt 13.4 User Argument Status

User-function argument preparation now has green generated PHPT coverage for:

- missing required arguments becoming catchable `ArgumentCountError`
- extra positional arguments ignored for binding but visible to
  `func_get_args()` and `func_num_args()`
- simple defaults
- variadic positional and named-tail packing
- by-value argument passing

The still-failing user-argument rows are not simple call binding failures. The
remaining focused blockers are advanced constant-expression defaults,
by-reference returns/sends, complex `isset`/`unset` lowering, Closure constant
expressions, and callable acquisition/parity errors.

## Prompt 13.5 By-Reference Send Status

By-reference parameter sends now have green generated PHPT coverage for:

- local variable sends through `IrCallArg.by_ref_local`
- array element sends through `IrCallArg.by_ref_dim`
- non-referenceable temporary mismatch as catchable `Error`

Ordinary object-property by-reference sends use the existing
`IrCallArg.by_ref_property` VM path when the property storage layer can expose a
reference cell. Unsupported property categories remain deterministic runtime
gaps instead of silent mutation, with existing IDs such as
`E_PHP_VM_BY_REF_PROPERTY_NON_OBJECT` and the property-reference known-gap IDs.

Remaining by-reference focused blockers are outside the 13.5 send MVP:
by-reference returns (`E_PHP_IR_UNSUPPORTED_BY_REF_RETURN` and
`E_PHP_VM_BY_REF_RETURN_*`) and callback/value-warning parity paths.

## Prompt 13.6 Closure Runtime Class Status

Closure runtime-class behavior now has green generated PHPT coverage for:

- `class_exists("Closure")` without a fake userland class definition
- first-class callable values exposing `Closure` object identity
- `instanceof Closure` and `Closure` parameter type checks
- direct invocation after `Closure` type binding
- `Closure::fromCallable()` returning an invocable `Closure`
- `var_dump($closure)` basic `object(Closure)#... (...)` shape
- direct `new Closure()` routing to the expected instantiation error

`Closure::bind`, `Closure::bindTo`, and `Closure::call` remain partial
runtime behaviors rather than complete Zend binding parity. The full closure
binding model is still tracked as
`E_PHP_RUNTIME_UNSUPPORTED_CLOSURE_BINDING`; Reflection parity for Closure
metadata remains outside this prompt.

## Prompt 13.7 Callable Acquisition Status

Callable acquisition and invocation now have green generated PHPT coverage for:

- plain function first-class callables and direct invocation
- instance method first-class callables
- static method first-class callables
- `call_user_func()` and `call_user_func_array()` over supported callable forms
- callable arrays with `[object, "method"]` and `[ClassName::class, "method"]`
- `is_callable()` for strings, closures, object-method arrays, static-method
  arrays, missing methods, and syntax-only checks
- callable parameter and return type checks over supported callable forms
- invalid array callback validation before `array_map()` iteration

Remaining callable gaps are concentrated in direct invalid callable-array call
parity. Unsupported direct calls still report stable VM diagnostics such as
`E_PHP_VM_INVALID_CALLABLE_ARRAY` rather than fully PHP-like catchable `TypeError`
wording in every path. First-class callable constant-expression forms and pipe
RHS callability remain separate lowering/acquisition blockers.

## Prompt 13.8 Scalar Coercion Status

Scalar parameter coercion now has green generated PHPT coverage for:

- weak-mode internal function coercion through generated arginfo
- strict-mode internal function rejection through generated arginfo
- weak-mode user-function scalar parameter coercion
- strict-mode user-function scalar parameter rejection as catchable `TypeError`

Runtime type checks use `IrUnit::strict_types` and route user parameter
mismatches through `E_PHP_VM_PARAM_TYPE_MISMATCH`, which maps to `TypeError`.
Internal builtins use generated `php_std::arginfo` metadata where custom
validation is not required. Remaining type-related `zend.functions` failures
are not simple scalar parameter coercion gaps; they are mostly relative type
context, advanced defaults, return variance wording, and callable/Closure
parity cases.

## Prompt 13.9 Pipe Callable Status

Pipe RHS dispatch now has green generated PHPT coverage for:

- first-class user-function callables
- closure values
- first-class internal builtins
- invalid non-callable RHS values as catchable `Error`

The VM pipe instruction routes RHS values through the existing callable
dispatcher and exception propagation path, preserving LHS evaluation before RHS
invocation. Remaining pipe-shaped focused failures are not ordinary pipe
callable-dispatch failures; the representative source cases feed `null` from
unsupported constant-expression/property-initializer paths before the pipe
executes.

## Prompt 13.10 Closeout

Prompt 13 closed with the selected generated `zend.functions` contract manifest
green at 29 PASS for both reference and target. The broader 200-row php-src
blocker slice remains 83 PASS and 117 FAIL on the target; those remaining rows
are outside the Prompt 13 contracts landed so far.

Before/after for the continued Prompt 13 slice:

| Scope | Before | After |
| --- | ---: | ---: |
| Generated PHPT manifest | 24 PASS / 0 non-green | 29 PASS / 0 non-green |
| Broader php-src reference slice | 200 PASS / 0 non-green | 200 PASS / 0 non-green |
| Broader php-src target slice | 83 PASS / 117 FAIL | 83 PASS / 117 FAIL |

No full-baseline fingerprint updates are accepted by this report. The module
manifest full-corpus counts remain 85 PASS, 53 SKIP, 727 FAIL, and 0 BORK from
887 corpus candidates.

The local full PHPT regression was available and ran with
`REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHPT_RUN_FULL=1`. It
completed 21,548 PHPTs with 2,454 PASS, 9,071 SKIP, 4 XFAIL, 9,879 FAIL, and
140 BORK, then failed baseline acceptance because 9 failure fingerprints were
new or changed:

- `Zend/tests/bug34260.phpt`
- `Zend/tests/bug48899.phpt`
- `Zend/tests/try/catch_finally_002.phpt`
- `Zend/tests/try/catch_finally_003.phpt`
- `ext/standard/tests/array/bug22463.phpt`
- `ext/standard/tests/array/bug34227.phpt`
- `ext/standard/tests/array/sort/bug50006.phpt`
- `ext/standard/tests/array/sort/bug50006_1.phpt`
- `ext/standard/tests/array/sort/bug50006_2.phpt`

Those fingerprints were not accepted into the tracked full-baseline manifests.

Recommendation: proceed to Prompt 14.1 by establishing the `zend.objects`
harness. Defer constant-expression closure/property-initializer work to a
dedicated frontend/IR slice; those rows now obscure callable progress more than
they exercise ordinary runtime callable dispatch.

## Top Primary Blockers

| Count | Primary blocker | Owner layer | Representative files |
| ---: | --- | --- | --- |
| 47 | `E_PHP_IR_UNSUPPORTED_HIR_STATEMENT` | `php_ir` / `php_semantics` | `Zend/tests/function_arguments/call_with_trailing_comma_basic.phpt`, `Zend/tests/first_class_callable/constexpr/userland.phpt`, `Zend/tests/closures/closure_const_expr/basic.phpt` |
| 19 | output mismatch without stable `E_PHP_*` id | parser / VM output parity | `Zend/tests/type_declarations/variance/return_type_will_change_function_error.phpt`, `Zend/tests/first_class_callable/first_class_callable_assert2.phpt`, `Zend/tests/first_class_callable/first_class_callable_008.phpt` |
| 8 | `E_PHP_VM_UNINITIALIZED_PROPERTY` | `php_vm` object/static initializer execution | `Zend/tests/first_class_callable/constexpr/property_initializer.phpt`, `Zend/tests/closures/closure_const_expr/property_initializer.phpt` |
| 5 | `E_PHP_IR_UNSUPPORTED_ADVANCED_PARAMETER` | `php_semantics` constant defaults / `php_ir` lowering | `Zend/tests/function_arguments/function_default_argument_cache.phpt`, `Zend/tests/closures/closure_const_expr/default_args.phpt` |
| 3 | pipe RHS receives `null` from unsupported constant-expression inputs | `php_ir` / `php_vm` callable acquisition | `Zend/tests/first_class_callable/constexpr/static_call_self.phpt`, `Zend/tests/closures/closure_const_expr/class_const.phpt` |
| 4 | `E_PHP_VM_UNKNOWN_METHOD` | `php_vm` closure/internal method dispatch | `Zend/tests/closures/closure_068.phpt`, `Zend/tests/closures/bug80929.phpt` |
| 3 | `E_PHP_RUNTIME_ERROR` | `php_runtime` callable comparison / closure semantics | `Zend/tests/closures/closure_compare.phpt`, `Zend/tests/closures/closure_015.phpt` |
| 2 | `E_PHP_INVALID_TYPE_SELF_CONTEXT` | `php_semantics` relative type context | `Zend/tests/type_declarations/relative_types/relative_type_in_closures.phpt`, `Zend/tests/type_declarations/relative_types/invalid_types/self_global_function.phpt` |
| 2 | `E_PHP_IR_UNSUPPORTED_CLASSLIKE_OBJECT` | `php_ir` class-like lowering | `Zend/tests/first_class_callable/first_class_callable_assert3.phpt`, `Zend/tests/closures/bug70397.phpt` |
| 2 | `E_PHP_IR_UNSUPPORTED_BY_REF_RETURN` | `php_ir` / `php_vm` by-reference returns | `Zend/tests/first_class_callable/first_class_callable_016.phpt`, `Zend/tests/closures/closure_014.phpt` |

## Hot Path Groups

| Count | Path group | Notes |
| ---: | --- | --- |
| 63 | `Zend/tests/closures` | Closure object metadata, closure const expressions, static/property initializer behavior, and Closure methods dominate. |
| 47 | `Zend/tests/first_class_callable` | First-class callable const expressions, callable acquisition errors, pipe RHS callability, and call-site error rendering dominate. |
| 5 | `Zend/tests/type_declarations` | Relative type context errors need PHP-like fatal wording and context handling. |
| 2 | `Zend/tests/function_arguments` | Advanced defaults and complex `isset`/`unset` lowering remain. |

## Suggested Next Slices

1. Land user-function argument semantics separately from Closure object
   metadata: defaults, named arguments, variadic packing, and surplus argument
   behavior now have a clearer builtin-arity baseline.
2. Keep Closure/internal callable work separate from const-expression lowering:
   the current focused failures show both, and mixing them would obscure
   ownership.
3. Treat output-mismatch-without-id rows as parser or rendering bugs first; do
   not convert them into known gaps without checking the reference file.
