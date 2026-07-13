# Trait Use HIR

The semantic frontend provides a semantic HIR view for trait-use declarations and adaptation
blocks without executing trait composition.

`HirTraitUse` records:

- the owning class-like ID;
- resolved trait names from the `use` clause;
- source-ordered adaptation entries.

`HirTraitAdaptation` records either:

- `precedence` for `TraitName::method insteadof OtherTrait`; or
- `alias` for `method as alias`, `method as private alias`, and visibility-only
  aliases such as `method as protected`.

The parser currently exposes adaptations as significant tokens inside
`TRAIT_USE_DECL`; the semantic lowerer builds conservative method references and
reports `E_PHP_TRAIT_ADAPTATION_INVALID_SHAPE` only for malformed shapes such as
missing method references, missing `insteadof` exclusions, or `as` entries with
no alias or visibility.

This layer does not compose traits, copy methods, resolve conflicts across
files, execute autoload-sensitive lookup, or validate runtime visibility
compatibility. Those checks remain deferred metadata for later semantic layers.
