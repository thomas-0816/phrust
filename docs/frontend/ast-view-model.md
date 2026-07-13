# AST View Model

`php_ast` provides typed views over the Parser CST. An AST view is a thin,
read-only wrapper around CST nodes and tokens. It gives semantic code a stable
way to ask for structured children without adding semantic behavior to the
parser.

## Principles

- Reuse `php_syntax` CST nodes and tokens.
- Preserve byte spans from the CST.
- Avoid panics on missing or malformed children.
- Represent missing children as absent views or explicit missing nodes in later
  HIR, not as parser rewrites.
- Keep lowering rules outside `php_syntax`.
- Do not evaluate expressions.

## Initial View Families

- `SourceFile`
- namespaces and use declarations
- constants, functions, parameters, and attributes
- classes, interfaces, traits, enums, anonymous classes, and members
- statements and control-flow syntax
- expressions, including PHP 8.5 pipe, void cast, clone-with, and first-class
  callable syntax
- type syntax, including nullable, union, intersection, DNF, and contextual
  names

## Identity

AST views should provide stable source identity through byte ranges and, where
needed, lightweight source AST IDs. Those IDs are local to a parsed source file
and are not a replacement for semantic symbol IDs.

## Core Surface

The initial `php_ast` core exposes:

- `AstNode` for typed CST node views.
- `AstToken` for typed CST token views.
- `AstPtr` as a source-local kind/range pointer.
- `SourceAstId` for caller-assigned source-local identity.
- direct child helpers for typed nodes and tokens.
- descendant helpers for typed nodes and tokens.
- `SourceFile`, `NamespaceDecl`, `UseDecl`, `ConstDecl`, `FunctionDecl`,
  `ClassDecl`, `InterfaceDecl`, `TraitDecl`, `EnumDecl`, member, parameter, and
  attribute views.

These views are structural only. They do not collect declarations, resolve
names, lower HIR, or emit semantic diagnostics.

## Declaration and Member Views

The declaration surface now includes:

- `Decl` for source-order declaration views.
- `UseGroup` over the current `USE_DECL` CST shape.
- `ClassLikeDecl` for class, interface, trait, and enum declarations.
- `MemberDecl` for methods, properties, class constants, and trait uses.
- `ParameterList` as the API alias for `ParamList`.
- `AnonymousClassDecl` for `CLASS_DECL` nodes that use anonymous-class syntax.
- `TraitUseDecl` adaptation helpers over the raw tokens inside a trait-use
  adaptation block.

The parser currently represents anonymous classes as nested `CLASS_DECL` nodes
under `NEW_EXPR`, and trait adaptations as tokens under `TRAIT_USE_DECL`.
`php_ast` exposes those shapes honestly instead of adding synthetic parser
nodes.

### SyntaxKind -> AstView

| SyntaxKind | Ast view |
| --- | --- |
| `SOURCE_FILE` | `SourceFile` |
| `NAMESPACE_STMT` | `NamespaceDecl` |
| `USE_DECL` | `UseDecl`, `UseGroup`, `UseItem` |
| `CONST_DECL` | `ConstDecl` |
| `FUNCTION_DECL` | `FunctionDecl` |
| `PARAM_LIST` | `ParameterList` |
| `PARAM` | `Parameter` |
| `CLASS_DECL` | `ClassDecl`, `AnonymousClassExpr`, `ExtendsClause`, `ImplementsClause` |
| `INTERFACE_DECL` | `InterfaceDecl`, `ExtendsClause` |
| `TRAIT_DECL` | `TraitDecl` |
| `ENUM_DECL` | `EnumDecl`, `ImplementsClause` |
| `CLASS_MEMBER_LIST` | class-like member iterator source |
| `METHOD_DECL` | `MethodDecl` |
| `PROPERTY_DECL` | `PropertyDecl`, `PropertyItem`, `EnumCase` when the node starts with `T_CASE` |
| `CLASS_CONST_DECL` | `ClassConstDecl` |
| `TRAIT_USE_DECL` | `TraitUse`, `TraitUseDecl`, `TraitAdaptation` |
| `ATTRIBUTE_GROUP` | `AttributeList` |
| `ATTRIBUTE` | `Attribute` |

`ExtendsClause`, `ImplementsClause`, `TraitAdaptation`, `UseItem`,
`PropertyItem`, and `EnumCase` reflect current Parser CST shapes. They do not
invent separate parser nodes or perform semantic validation.

## Statement, Expression, and Type Views

The AST surface now includes structural views for the parser's statement,
expression, type, and PHP 8.5 node families:

- `Stmt` covers inline HTML, empty, expression, echo, return, throw, break,
  continue, block, if, loop, switch, try/catch/finally, declare, global,
  static, unset, goto, and label statement nodes.
- `ExprNode` covers literals, variables, names, parenthesized expressions,
  prefix/postfix expressions, binary/assignment/ternary expressions, calls,
  fetches, arrays, match, throw, constructs, yield, closures, arrow functions,
  `new`, `clone`, PHP 8.5 `clone(...)`, PHP 8.5 pipes, strings, encapsed
  strings, and heredocs.
- `TypeView` covers generic, nullable, union, intersection, and DNF-shaped type
  syntax nodes.

The current parser does not create a separate `MatchArm` node. Match arms remain
raw expression and `T_DOUBLE_ARROW` token sequences under `MATCH_EXPR`, so
`php_ast` exposes `MatchExpr` rather than inventing synthetic arm nodes.

AST compatibility names are provided where the CST intentionally uses
a broader parser node:

- `ExpressionStmt` aliases `ExprStmt`.
- `LiteralExpr`, `VariableExpr`, and `NameExpr` alias the current literal,
  variable, and name nodes.
- `UnaryExpr` aliases `PrefixExpr`; `CastExpr` aliases `PrefixExpr`, with
  `VoidCastExpr` remaining a distinct PHP 8.5 node.
- `CoalesceExpr` aliases `BinaryExpr`; use `BinaryExpr::is_coalesce()` to
  identify `??`.
- `ListExpr` aliases `ArrayExpr`; use `ArrayExpr::is_list_syntax()`.
- method, nullsafe method, and nullsafe property fetch aliases all wrap
  `PropertyFetchExpr`; use `PropertyFetchExpr::is_nullsafe()` for `?->`.
- `FirstClassCallableExpr` aliases `CallExpr`; use
  `CallExpr::is_first_class_callable()` for `...`.
- include, eval, and exit expression aliases wrap `ConstructExpr`; use
  `ConstructExpr::construct_kind()`.
- keyword type aliases such as `VoidType`, `NeverType`, `StaticType`,
  `SelfType`, `ParentType`, `FalseType`, `TrueType`, `NullType`, `MixedType`,
  `IterableType`, `ObjectType`, and `CallableType` wrap `TypeNode`; use
  `TypeNode::keyword()` or `TypeView::keyword()`.

These APIs still do not emit semantic errors. Broken or incomplete syntax is
handled defensively by returning absent views or narrower classifier results.
