# Handoff to Semantic Layers

The parser and CST provide syntax structure only. Later semantic work should
consume the CST and build higher-level views without changing parser
correctness requirements.

## Inputs Provided by the Parser

- Lossless CST nodes and tokens.
- Stable byte ranges for nodes, tokens, and diagnostics.
- Optional parse-result source identity through `ParseContext`.
- Parser diagnostics and error nodes.
- Fixture and reference acceptance data.
- Documented known gaps.

## Work for the Next Layer

- AST/HIR or typed-AST views over CST nodes.
- Declaration tables for functions, classes, interfaces, traits, enums,
  constants, properties, methods, parameters, and attributes.
- Namespace and name resolution for declarations, imports, fully qualified
  names, relative names, `self`, `parent`, and `static`.
- Compile-time checks that PHP lint reports beyond grammar acceptance, including
  duplicate declarations in one scope, invalid modifier combinations, abstract
  member rules, and context-sensitive statement restrictions.
- Constant expression handling for defaults, attributes, enum cases, class
  constants, and static initializers.
- Attribute lowering from syntax nodes to validated attribute metadata,
  including target validation and argument expression preparation.
- Type model construction for named, nullable, union, intersection,
  parenthesized DNF, builtin, `self`, `parent`, `static`, callable, and
  iterable forms.
- Bytecode/IR preparation that consumes semantic views, not raw parser events,
  and keeps execution/runtime behavior outside the parser crate.
- Optional LSP/IDE layers that assign stable node identity and incremental
  reparse caches around statement, class-member, and function-body boundaries.

## Executable Starting Point

Start semantic work by consuming the existing parser API:

```rust
use php_syntax::parse_source_file;

let parse = parse_source_file(source);
let root = parse.root();
let diagnostics = parse.diagnostics();
```

The first semantic pass should be a read-only CST walk that builds declaration
tables and preserves parser diagnostics unchanged. Add new diagnostics in a
separate semantic diagnostic type so parser acceptance remains comparable with
the PHP lint oracle.

Recommended first vertical slice:

1. Add a new semantic crate or module that depends on `php_syntax`.
2. Walk `SOURCE_FILE` and declaration/member nodes without mutating the CST.
3. Build declaration tables for functions, classes, interfaces, traits, enums,
   constants, properties, methods, and parameters.
4. Emit semantic diagnostics separately from `ParseDiagnostic`.
5. Prove the slice with focused fixtures and keep `just parser-diff` and
   `just cst-roundtrip` green.

## Boundary

Semantic layers may reject syntax accepted by the parser. The parser should not
perform name resolution, type checking, constant evaluation, or runtime
execution.

Incremental reparsing is also a later tooling concern. It should consume the
lossless CST and byte ranges, then choose conservative invalidation boundaries.
Encapsed strings and heredocs need special treatment because lexer mode changes
can affect tokenization beyond the edited byte span.
