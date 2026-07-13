# Attributes

Attributes are lowered into metadata attached to declarations, parameters,
members, and other supported targets.

## Lowered Data

- target node
- resolved attribute name where possible
- source span
- argument expressions
- constant-expression validation result

## Boundary

Semantic frontend does not instantiate attribute classes, call constructors, autoload
classes, or execute argument expressions. Target validation and syntax-level
argument validation are semantic diagnostics.

## Fixtures

The semantic frontend provides:

- `fixtures/semantic/attributes/class.php`
- `fixtures/semantic/attributes/function.php`
- `fixtures/semantic/attributes/method-param-property.php`
- `fixtures/semantic/attributes/enum-case.php`
- `fixtures/semantic/attributes/invalid-argument-call.php`
- `fixtures/semantic/attributes/php85-closure-argument.php`
