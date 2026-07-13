# Class Member HIR

The semantic frontend provides typed member records to the semantic frontend without executing
class semantics.

The `php_semantics` HIR module now stores:

- `HirMethod` records for class-like methods, with the owning class-like ID,
  modifier set, body presence, attached attributes, and the existing lowered
  signature index.
- `HirProperty` records for property declarations, including property items,
  lowered type IDs, default constant-expression IDs when already collected,
  property-hook summaries, modifiers, and attributes.
- `HirClassConst` records for class constants, including type IDs,
  initializer constant-expression IDs, modifiers, and attributes.

`HirClassLike::members()` remains the source-order summary, but method,
property, and class-constant summaries now carry typed member IDs so consumers
can jump from class structure to the detailed member record.

Validation added in this scope is structural and per class-like declaration:

- duplicate method names are checked case-insensitively;
- duplicate property names are checked by source variable name;
- duplicate class constant names are checked by source name.

Parser acceptance and reference behavior stay bounded by the PHP lint oracle.
No autoload-sensitive lookup, method body execution, VM behavior, or class
member compatibility analysis is performed in this frontend work.
