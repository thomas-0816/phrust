# Parser Known Gaps

This file tracks only accepted, versioned parser/reference gaps. The executable
allowlist is `fixtures/parser/known_gaps.toml`, and this document must agree
with it exactly.

There are no accepted parser gaps for the curated fixture suite at this point.
An empty `fixtures/parser/known_gaps.toml` means every fixture mismatch is a
failure.

Optional corpus-smoke deviations are not accepted gaps. Reduce any real corpus
issue to a curated fixture before adding it here.

## Required Entry Format

```text
ID:
Beschreibung:
Beispiel:
Referenzverhalten:
Rust-Verhalten:
Priorität:
Geplanter Folgeprompt/Phase:
```
