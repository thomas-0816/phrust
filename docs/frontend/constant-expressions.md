# Constant Expressions

Semantic frontend validates constant-expression contexts structurally. It records HIR
expressions as `ConstExpr` candidates and checks whether the expression form is
allowed in the context. It does not evaluate values, instantiate objects, run
functions, resolve autoloading, or model zvals.

## Contexts

- global constant initializers
- class constant initializers
- enum case values
- parameter defaults
- attribute arguments
- static local initializers
- property defaults
- promoted property defaults

Enum case backing values use the same non-evaluating structural validation as
other constant-expression contexts.

## HIR Records

`hir::const_expr` defines:

- `ConstExprId`
- `ConstExpr`
- `ConstExprKind`
- `ConstExprContext`

`module.const_exprs` in frontend JSON contains:

- `id`
- `context`
- `kind`
- `expr_id`
- `allowed`
- `span`

The `expr_id` links the candidate back to the structural expression HIR. The
span is byte based and comes from the CST expression candidate.

## Allowed Forms

The validator accepts these forms recursively:

- scalar literals and visible magic-constant-like literal forms
- constant name fetches
- class constant fetches, including enum-case-like static fetches when exposed
- arrays and list-like array records
- unary, binary, ternary, and coalesce-like binary expressions
- closures and arrow functions
- first-class callables
- casts
- `new` expressions when the class and constructor arguments are structurally
  allowed constant expressions

PHP 8.5.7 reference checks confirmed closures in parameter defaults,
first-class callables, casts, and `new` in constant-expression contexts. The
fixtures under `fixtures/semantic/const_expr/` document those accepted forms.

## Disallowed Forms

The validator rejects these forms without evaluation:

- variable fetches
- normal function calls
- method calls
- property fetches
- assignments
- yield and yield from
- include, require, require_once, include_once
- eval
- clone and clone-with
- pipe expressions
- match expressions
- array dimension fetches
- unlowered or missing expression placeholders

Disallowed candidates emit `E_PHP_INVALID_CONST_EXPR`, except attribute
arguments, which emit `E_PHP_ATTRIBUTE_ARGUMENT_NOT_CONST_EXPR`.

## Fixtures

The semantic frontend provides:

- `fixtures/semantic/const_expr/scalars.php`
- `fixtures/semantic/const_expr/arrays.php`
- `fixtures/semantic/const_expr/class_const_fetch.php`
- `fixtures/semantic/const_expr/enum_case.php`
- `fixtures/semantic/const_expr/invalid_variable.php`
- `fixtures/semantic/const_expr/invalid_call.php`
- `fixtures/semantic/const_expr/php85_closure.php`
- `fixtures/semantic/const_expr/php85_first_class_callable.php`
- `fixtures/semantic/const_expr/php85_cast.php`
- `fixtures/semantic/const_expr/php85_new.php`

The invalid fixtures are expected Rust and PHP-reference rejects. The accepted
fixtures are analyzed only; no runtime work is performed.

## Conservative Folding

The semantic frontend provides optional pure folding to `ConstExpr` candidates. Folded values
are exposed as `folded_value` in frontend JSON when the expression is proven
without runtime semantics. This value is not a PHP zval and is not used for
execution.

The folder currently folds:

- `null`, boolean, integer, and string literals
- unary `+` and `-` on folded integer literals
- string concatenation of folded string literals
- unresolved constant-name references as symbolic references

The folder intentionally does not fold floats, magic constants, class constant
fetches, arrays, calls, `new`, closures, first-class callables, or anything
that would require runtime lookup or object/value semantics.

The semantic frontend provides:

- `fixtures/semantic/const_expr/fold_literals.php`
- `fixtures/semantic/const_expr/fold_concat.php`
- `fixtures/semantic/const_expr/no_fold_class_const.php`
- `fixtures/semantic/const_expr/no_fold_magic_constant.php`
