# Expression and Statement HIR

The semantic frontend lowers expressions and statements into structural HIR. The lowering
is intentionally non-executing: includes, eval, exit, calls, object creation,
and PHP 8.5 forms are represented as nodes only.

## Statements

`HirStmtKind` covers expression statements, blocks, if/while/do/for/foreach,
switch, try/catch/finally blocks, return, throw, break, continue, declare,
global, static, unset, echo, inline HTML, labels, and goto.

Every lowered statement is stored in `HirModule::statements()` and receives a
source-map span. Statement children reference `StmtId` values; statement
headers reference `ExprId` values when the parser exposes expression nodes.

The current parser keeps some control-flow headers as token-only CST regions.
For those valid inputs, HIR records a `Missing` expression placeholder with a
source span but does not emit a semantic error. True recovery cases, such as an
empty `echo`, emit `E_PHP_HIR_MISSING_CHILD`.

## Expressions

`HirExprKind` covers missing nodes, literals, variables, names, arrays, lists,
unary and binary forms, assignments, ternaries, calls, method calls, property
fetches, static access, dimension fetches, closures, arrow functions, `new`,
`clone`, PHP 8.5 clone-with, match, yield, yield from, include/require,
eval, exit/die, casts, pipe expressions, and first-class callables. Include,
require, and eval nodes carry deferred-effect metadata for possible file loads,
symbol definitions, runtime code execution, and current-scope effects.

Every lowered expression is stored in `HirModule::expressions()` and receives a
source-map span. Child links use `ExprId` values; missing children use
`HirExprKind::Missing` rather than panicking.

## Name Resolution Annotations

Name expressions carry `HirNameResolution` metadata:

- source spelling
- resolution context
- classification
- resolved candidate
- optional runtime fallback candidate

Function and constant fallback is represented as `maybe_runtime_fallback` when
PHP lookup may defer from a namespaced candidate to a global function or
constant. The frontend records this fallback; it does not decide runtime
availability.

## JSON Output

The CLI `analyze --format json` output now includes:

- `module.statements`
- `module.expressions`

Each node includes its stable arena ID, kind, source span, and structural child
IDs or metadata. This output is a debug/snapshot surface, not a bytecode or
runtime format.

## Boundaries

This pass does not:

- evaluate constants or expressions
- execute includes, eval, or exit
- instantiate objects
- resolve methods, properties, or calls semantically
- introduce bytecode, VM, JIT, or runtime values
