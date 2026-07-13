# Scope Model

Semantic frontend scopes describe compile-time nesting and context. They are not runtime
activation records and do not store runtime variable values.

## Scope Kinds

- file
- namespace
- class
- interface
- trait
- enum
- function
- method
- closure
- arrow function
No local variable block scope is created for ordinary PHP statement blocks. PHP
local variables are function-like scoped, unlike Rust block-scoped locals.

## Tracked Context

- current namespace
- current class-like declaration
- current function-like declaration
- loop and switch depth
- function-like parameter declarations
- closure `use` variables, as explicit capture metadata only
- arrow-function implicit capture marker:
  `implicit_by_value_deferred`
- `global` statements
- `static` local declarations
- generator/yield presence
- `$this`, `self`, `parent`, and `static` availability
- deferred include/eval markers

Class scopes and method variable scopes are separate. A method scope is a child
of its class-like scope, but local variables belong to the method function-like
scope, not the class scope.

`$this`, `self`, `parent`, and `static` require later context checks. The semantic frontend records the relevant enclosing scopes but does not validate those references.

The scope model supports compile-time diagnostics such as invalid
break/continue, invalid return/yield context, invalid `$this`, and invalid
`self`/`parent`/`static` references. Full control-flow graph validation and
runtime capture evaluation are later layers unless a Semantic frontend fixture explicitly
requires them.

## CLI

The frontend CLI can render the scope tree:

```bash
php-frontend scopes fixtures/semantic/scopes/closure-use.php --format text
```

The text output is stable enough for human inspection and shows scope kind,
ID, optional name, parameters, captures, and `global`/`static` metadata.
