# Expression Precedence

Expressions are parsed with a precedence-climbing parser. The current parser
covers primary, prefix, common binary expressions, assignment, ternary/elvis,
pipe, coalesce, and yield forms. Runtime and generator semantics are not part of
the parser.

## Primary

- literals: integers, floats, constant strings, magic constants
- pseudo-literals: `null`, `true`, `false` as `T_STRING` text
- variables: `$x`
- names: `Foo`, `Foo\Bar`, `\Foo\Bar`, `namespace\Foo`
- parenthesized expressions

## Prefix

Prefix expressions bind tighter than multiplicative/additive expressions and
looser than exponentiation, matching PHP's special `**` behavior.

- `+`
- `-`
- `!`
- `~`
- `@`
- cast tokens emitted by the lexer: `(int)`, `(float)`, `(string)`, `(array)`,
  `(object)`, `(bool)`, `(unset)`, `(void)`

## Binary Binding Powers

Higher binding power binds tighter.

| Operators | Associativity | Binding power |
| --- | --- | --- |
| `**` | right | 130 |
| `*`, `/`, `%` | left | 120 |
| `|>` | left | 115 |
| `+`, `-` | left | 110 |
| `.` | left | 100 |
| `<<`, `>>` | left | 90 |
| `<`, `>`, `<=`, `>=`, `<=>` | left | 80 |
| `==`, `!=`, `===`, `!==` | left | 70 |
| `&&` | left | 60 |
| `||` | left | 50 |
| `??` | right | 40 |
| `and` | left | 30 |
| `xor` | left | 20 |
| `or` | left | 10 |
| `?:`, `? :` | right-ish parser form | 6 |
| `=`, `+=`, `-=`, `*=`, `/=`, `%=`, `.=` | right | 5 |
| `&=`, `|=`, `^=`, `<<=`, `>>=`, `??=` | right | 5 |

`**`, `??`, and assignment operators use equal left/right binding power so the
right operand can contain the same operator, producing right-associative
grouping.

## Pipe

The PHP 8.5 `|>` token is modeled as a dedicated `PIPE_EXPR` node instead of a
generic `BINARY_EXPR`. This keeps it visible for later semantic validation while
still using the precedence table for grouping. The parser only models syntax; it
does not validate callable semantics or execute the pipe.

## Yield

`yield`, `yield key => value`, and `yield from expr` produce `YIELD_EXPR` nodes.
The parser does not implement generator runtime behavior.

## Recovery

Expression parsing stops on statement recovery tokens, commas for echo operand
lists, closing parentheses, and obvious next statement starters such as `echo`.
This keeps malformed expressions bounded so statement recovery can emit
terminator diagnostics instead of consuming the rest of the file.
