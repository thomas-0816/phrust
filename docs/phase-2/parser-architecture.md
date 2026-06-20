# Parser Architecture

The parser pipeline is:

```text
SourceText
  -> php_lexer token stream
  -> TokenSource
  -> event-based Parser
  -> TreeSink
  -> lossless CST
```

`SourceText` and byte spans remain the source of truth. The lexer is responsible
for tokenization and mode handling. The parser consumes lexer tokens through a
small `TokenSource` facade and records structural events rather than allocating
final tree nodes directly during grammar descent. A `TreeSink` later turns those
events into CST nodes and tokens.

The public parse API accepts `&str` and also offers a `SourceText` convenience
entry point. Parse results may carry an optional caller-owned `SourceId` through
`ParseContext`; the parser does not allocate global file tables, mutable
singletons, or host-specific file identifiers. This keeps engine use simple
while leaving room for an LSP host to map parse diagnostics and CST ranges back
to its own file model.

`TokenSource` does not reclassify tokens. It exposes `current()`,
`current_text()`, `current_range()`, and the small
`current_keyword_context()` bridge for grammar sites that need PHP's contextual
identifier behavior. The bridge reports the lexer token name plus source text
for keyword-like tokens; grammar code must still opt in at positions where PHP
allows reserved words as names. Current opt-in sites include object and static
member access such as `$object->match()` and `ClassName::readonly`.

The reference `TOKEN_PARSE` mode can turn some reserved words into `T_STRING`
after parser context is known. This project keeps the lexer output purely
lexical and handles the same acceptance in parser grammar positions. That keeps
Phase 1 token comparison stable and avoids a second parser-aware lexer.

## Why Event-Based Parsing

An event stream lets grammar functions mark nodes before their final extent is
known, recover from errors, and keep tree construction separate from parsing
control flow. This matches the needs of PHP syntax, where declarations,
expressions, strings, and inline HTML all need structured recovery without
losing source bytes.

The current parser core emits these events:

```text
Placeholder
StartNode(SyntaxKind)
AddToken
Error(ParseDiagnostic)
FinishNode
```

`Marker` reserves a start event slot and `CompletedMarker` records that the
slot was completed with a concrete node kind. `Parser::bump()` is the only path
that emits `AddToken`, and it always advances the token cursor when not at EOF.
Recovery helpers must either consume at least one token or stop at EOF.

The current grammar builds nested source-file, PHP block, statement,
declaration, type, expression, class-member, attribute, string, heredoc, and
error-recovery nodes. It is still syntactic: semantic validation is deliberately
left to later layers.

## Why the CST Is Lossless

The parser is a core engine component and a future tooling foundation. It must
retain comments, whitespace, PHP open/close tags, inline HTML, and exact token
text so later layers can reconstruct, inspect, or map diagnostics back to the
original file. No parser step may normalize source text.

Roundtrip invariant:

```text
source bytes == concatenate(all CST token texts)
```

Every CST node and token exposes its byte `TextRange` through `range()` and
`text_range()`. Ranges are half-open and byte-based. The current tree is rebuilt
per parse; there is no stable node ID across parses yet, but the immutable
node/token storage and caller-owned source identity avoid decisions that would
block future stable identity layers.

## Incremental Parsing Readiness

The parser does not implement diff-based incremental reparsing yet. A future
LSP or IDE layer should be able to add it above or behind the current public
surface because:

- parse results do not depend on global mutable state,
- source identity is optional metadata carried by `ParseContext`,
- nodes and tokens carry byte ranges,
- parsing a changed source string creates a fresh independent CST,
- CST reconstruction remains the correctness invariant.

Likely future reparse boundaries are:

- statement nodes in `STATEMENT_LIST`,
- class members inside `CLASS_MEMBER_LIST`,
- function, method, and closure bodies,
- attribute groups as declaration-leading syntax,
- encapsed string and heredoc bodies as special lexer-mode regions.

Encapsed strings and heredocs should be treated conservatively because edits can
change lexer mode and token boundaries across lines. The first incremental
implementation should prefer a wider boundary there rather than preserving stale
subtrees.

## Performance Baseline

Parser performance is tracked with a lightweight, explicit smoke command:

```bash
nix develop -c just bench-parser
```

The smoke parses four synthetic inputs:

- a small file,
- an expression-heavy file,
- a class/member-heavy file,
- a heredoc/string-heavy file.

For each input it prints parse time, source byte length, CST node count, CST
token count, and diagnostic count. The test also asserts lossless
reconstruction so benchmark-only changes cannot silently weaken correctness.

This command is not part of the hard verification gates. It is a local baseline
tool for spotting large regressions before later optimization work. No parser
API should be complicated only to improve this smoke; correctness and PHP
compatibility stay ahead of throughput tuning.

## Syntax vs Semantics

The parser accepts syntax. Later semantic layers decide whether accepted syntax
is meaningful or legal in a particular compile-time context. Examples that
belong outside the parser include duplicate parameter checks, name resolution,
abstract method rules, type compatibility, constant-expression evaluation, and
attribute target validation.

Keeping syntax and semantics separate avoids encoding PHP compiler policy into
the CST builder and keeps error recovery predictable.

Contextual keywords follow the same rule. The parser may accept a keyword token
as a member name where PHP syntax allows it, but it does not decide whether a
member exists, whether a class constant is valid for runtime access, or whether
the reference engine would later issue a semantic diagnostic.
