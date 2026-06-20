# Grammar Coverage

Coverage is tracked by syntax area, curated fixtures, and reference comparison.
The table starts as a contract and must be kept current as parser support lands.

| Syntaxbereich | Status | Fixtures | Referenzvergleich | Notizen |
| --- | --- | --- | --- | --- |
| PHP/HTML source files | implemented | `valid/pure_html.php`, `valid/inline_html.php`, `valid/multiple_php_blocks.php`, `valid/short_echo_tag.php`, `valid/php_html_modes.php` | `php -l` acceptance compared by `just parser-diff` | Open/close tags, pure inline HTML, multiple PHP blocks, close/open transitions, and short echo tags are retained losslessly. |
| Basic statements | implemented | `valid/statements_basic.php`, `invalid/statements_missing_semicolon.php`, `invalid/missing_semicolon.php` | `php -l` acceptance compared by `just parser-diff` | Empty, block, echo, expression statements, close-tag terminators, and missing-terminator diagnostics. |
| Control flow | implemented | `valid/control_flow.php`, `valid/alternative_syntax.php`, `valid/match_expression.php`, `invalid/control_flow_invalid.php` | `php -l` acceptance compared by `just parser-diff` | Brace and alternative `if`/loop/switch syntax, foreach by-reference headers, break/continue/return/throw statements, and match expressions are parsed syntactically. Flow reachability and nesting semantics remain outside parser scope. |
| Expressions | implemented | `valid/expressions_basic.php`, `valid/operator_groups.php`, `invalid/expressions_basic_invalid.php` | `php -l` acceptance compared by `just parser-diff` | Pratt parser covers prefix, arithmetic, power, concat, shift, comparison, equality, bitwise, boolean, keyword logical, coalesce, casts, and names. |
| Assignments and yield | implemented | `valid/expressions_assignment_ternary.php`, `valid/generators_yield.php` | `php -l` acceptance compared by `just parser-diff` | Assignment chains, compound/coalesce assignment, ternary/elvis, coalesce, `yield`, keyed `yield`, and `yield from`. |
| Calls and postfix chains | implemented | `valid/expressions_postfix.php`, `valid/first_class_callable.php`, `invalid/expressions_postfix_invalid.php` | `php -l` acceptance compared by `just parser-diff` | Calls, named/spread args, dims, property/nullsafe fetch, static access, and first-class callable syntax. |
| Arrays and destructuring | implemented | `valid/arrays.php`, `valid/destructuring.php`, `invalid/arrays_invalid.php` | `php -l` acceptance compared by `just parser-diff` | Short arrays, `array()`, `list()`, keyed elements, references, spread, nesting, and destructuring assignment. |
| Functions and closures | implemented | `valid/functions.php`, `valid/closures.php`, `valid/arrow_functions.php`, `invalid/functions_invalid.php` | `php -l` acceptance compared by `just parser-diff` | Declarations, closures, static closures, arrows, parameters, return-by-reference, by-reference params, variadics, defaults, and closure use lists. Binding and scope semantics are intentionally out of parser scope. |
| Namespaces and use | implemented | `valid/namespaces.php`, `valid/use_declarations.php`, `invalid/namespaces_invalid.php` | `php -l` acceptance compared by `just parser-diff` | Bracketed/global and unbracketed namespaces, function/const/class imports, group use, mixed group entries, and qualified-name tokens. No name resolution. PHP lint rejects files that mix bracketed and unbracketed namespace declarations, so that semantic boundary is not modeled by the parser. |
| Attributes | implemented | `valid/attributes.php`, `invalid/attributes_invalid.php`, `recovery/bad_attribute.php` | `php -l` acceptance compared by `just parser-diff` | Attribute groups, multiple attributes, qualified names, balanced argument lists, declaration/parameter/member attachment points, and malformed group recovery. Target validation remains semantic work. |
| Type syntax | implemented | `valid/types.php`, `valid/dnf_types.php`, `invalid/types_invalid.php` | `php -l` acceptance compared by `just parser-diff` | Nullable, union, intersection, parenthesized DNF, callable/static/self/parent and builtin-name syntax. Parser recovery stops at parameter/member delimiters; semantic restrictions such as invalid builtin combinations remain outside syntax unless PHP lint rejects the fixture. |
| Classes/interfaces/traits/enums | implemented | `valid/classes_basic.php`, `valid/interfaces_traits.php`, `valid/enums.php`, `invalid/classes_invalid.php` | `php -l` acceptance compared by `just parser-diff` | Class/interface/trait/enum heads, extends/implements surfaces, shallow member lists, trait-use members, enum cases, and anonymous `new class` expressions. Member semantics are not validated. |
| Class members | implemented | `valid/class_members.php`, `valid/property_hooks.php`, `valid/promoted_properties.php`, `invalid/class_members_invalid.php` | `php -l` acceptance compared by `just parser-diff` | Properties, typed constants, methods, trait adaptations, constructor-promoted parameters, asymmetric visibility tokens, and property-hook bodies are parsed syntactically. Modifier validity and hook semantics remain out of scope. |
| Exception and misc statements | implemented | `valid/statements_misc.php`, `valid/try_catch_finally.php`, `valid/declare.php`, `invalid/statements_misc_invalid.php` | `php -l` acceptance compared by `just parser-diff` | Try/catch/finally, declare block and alternative forms, include/require/print/isset/empty/eval/exit constructs, global/static/unset, labels, and goto are parsed syntactically. Include effects, scope behavior, and control-transfer semantics are out of scope. |
| Strings and heredoc | in progress | `valid/strings.php`, `valid/encapsed_strings.php`, `valid/heredoc_nowdoc.php`, `invalid/strings_invalid.php` | `php -l` acceptance compared by `just parser-diff` | Constant strings, interpolated double/backtick strings, heredoc, nowdoc, simple interpolation, and braced interpolation are grouped into lossless CST nodes. Complex interpolation contents remain shallow because the lexer supplies interpolation-mode tokens rather than full expression-mode tokenization. |
| PHP 8.5 syntax forms | implemented | `valid/php85/pipe_operator.php`, `valid/php85/clone_with.php`, `valid/php85/void_cast.php`, `valid/php85/constant_expressions.php`, `php85/syntax_matrix.php`, `invalid/php85_invalid.php`, `invalid/pipe_operator_invalid.php` | `php -l` acceptance compared by `just parser-diff` | Pipe expressions use `PIPE_EXPR`; `(void)` uses `VOID_CAST_EXPR`; PHP 8.5 clone-with is reference-modeled as the `clone(...)` language construct argument list; static closures, casts, first-class callables, and `#[\NoDiscard]` attributes in constant-expression contexts are parsed syntactically. |

## Syntax Kind Surface

The CST kind taxonomy is split into `SyntaxKind::Token(SyntaxTokenKind)` and
`SyntaxKind::Node(SyntaxNodeKind)`.

Token coverage starts from the lexer token surface: PHP open/close tags, inline
HTML, trivia tokens, names, variables, keywords, literals, encapsed parts,
heredoc/nowdoc markers, operators, punctuators, and PHP 8.5 tokens such as
`T_PIPE` and `T_VOID_CAST`.

Node coverage is declared ahead of grammar implementation for source files, PHP
blocks, statement lists, declarations, class members, attributes, types,
expressions, strings, encapsed strings, heredoc, and error recovery nodes.
