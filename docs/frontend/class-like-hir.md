# Class-Like HIR

The semantic frontend provides structural HIR for classes, interfaces, traits, enums, and
anonymous classes.

`HirClassLike` records:

- kind: class, interface, trait, enum, or anonymous class
- source name and FQN for named declarations
- stable local anonymous ID for anonymous classes
- modifier flags through `ModifierSet`
- resolved `extends`, `implements`, and trait-use names
- structural member summaries
- attached attribute IDs
- enum backing type ID when present

This layer does not autoload classes, resolve inheritance across files, flatten
traits, or execute enum/class runtime semantics. Member-specific HIR is added by
the following class-member work items.
