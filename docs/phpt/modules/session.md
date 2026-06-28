# session

- Strategy: classify, no CLI session MVP yet
- Classification: real-implementation-required for framework session support
- Selected manifest: `tests/phpt/manifests/modules/session.selected.jsonl`
- Current corpus snapshot: 260 `session` candidates, 3 PASS, 0 SKIP, 254 FAIL,
  2 BORK, and 260 known non-green outcomes.

## Decision

Do not implement session state in this branch.

A deterministic CLI-only MVP is possible later, but it must be request-local
state integrated with superglobals, INI options, serialization, filesystem
storage policy, and warning/error behavior. This branch does not add partial
`$_SESSION` mutation or fake persistence. Platform probes stay negative.

## Unsupported Area

- Stable ID: `PHPT-DATA-SESSION`
- Reference behavior: PHP with `session` enabled exposes `session_start`,
  `session_id`, `session_status`, `$_SESSION`, handlers, serializers, INI
  options, and request lifecycle behavior.
- Current phrust behavior: `extension_loaded("session")`,
  `function_exists("session_start")`, and `class_exists("SessionHandler")` are
  false; no session lifecycle exists.
- Fixture: `tests/phpt/generated/session/platform-checks.phpt`
- Next owner layer: future request/runtime state layer after filesystem and
  superglobal behavior are ready.

## Non-Scope

- HTTP cookies
- web SAPI lifecycle
- uploads/request lifecycle
- full session handler matrix

## Source References

- `ext/session/session.stub.php`
- `ext/session/tests/`

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=session`
- `nix develop -c just verify-stdlib` if runtime code changes
- `nix develop -c just verify-phpt`
