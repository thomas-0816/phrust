# Fixture Conventions

Phase 0 keeps this directory empty except for documentation and `.gitkeep`
markers.

Future fixture names should follow:

- `NNNN_feature_name.php`
- `NNNN_feature_name.expected.tokens.json`
- `NNNN_feature_name.expected.parse.json`
- `NNNN_feature_name.expected.stdout`

Expected files must be generated from the pinned PHP `8.5.7` reference where
possible. Hand-authored expectations need provenance notes and review.
