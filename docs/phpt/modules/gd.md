# gd

- Strategy: classify, do not implement image processing
- Classification: out-of-scope
- Selected manifest: `tests/phpt/manifests/modules/gd.selected.jsonl`
- Current corpus snapshot: 312 `gd` candidates, 1 PASS, 55 SKIP, 255 FAIL,
  0 BORK, and 310 known non-green outcomes.

## Decision

Keep GD out of scope for this branch.

GD requires image decoders/encoders, drawing primitives, color management,
resource/object modeling, binary output parity, and image-library dependencies.
This branch must not start a graphics subsystem. Platform probes remain
negative so applications do not assume image processing is available.

## Unsupported Area

- Stable ID: `PHPT-DATA-GD`
- Reference behavior: PHP with GD enabled exposes `GdImage`, image creation,
  decoding/encoding, drawing, filters, font handling, metadata, and many format
  conditionals.
- Current phrust behavior: `extension_loaded("gd")`,
  `class_exists("GdImage")`, and representative `image*` functions are false.
- Fixture: `tests/phpt/generated/gd/platform-checks.phpt`
- Next owner layer: future optional graphics extension with explicit image
  library policy.

## Source References

- `ext/gd/gd.stub.php`
- `ext/gd/tests/`

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=gd`
- `nix develop -c just verify-phpt`
