# Persistent Type Feedback and Invalidation

FPE-20 adds the first engine-owned persistent feedback contract. Metadata is
loaded, validated, and reported; validator-accepted quickening sites and
monomorphic entry-unit function callsites may also seed the next run's
adaptive state. Consumption is governed by a dedicated policy
(`--persistent-feedback-consume=off|quickening|quickening-ics`,
`PHRUST_PERSISTENT_FEEDBACK_CONSUME`) that is separate from reading, writing,
and stats. The `php-vm run` default follows the sidecar default (consume on,
like the bytecode cache, in the full `quickening-ics` mode); seeded sites
always run behind the full runtime guard protocol — quickening seeds
self-correct through dequickening, and seeded IC entries validate name, arity
shape, and observation epoch at the callsite before dispatch — so a stale
seed never changes PHP-visible behavior.

## Key Model

Every feedback entry is keyed by:

- source fingerprint from the same cache-fingerprint machinery used by the
  bytecode-cache envelope;
- engine version and PHP target version;
- compile options, including opt level, execution format, quickening, inline
  caches, bytecode cache mode, JIT mode, and tiering mode;
- function ID and instruction ID;
- IR fingerprint over the current IR snapshot;
- class-table, function-table, autoload, and include-path epochs;
- target architecture/config label.

Any mismatch rejects the entry as stale. Stale entries are counted and the run
continues through baseline execution.

## Metadata Only

The persistent payload can represent:

- monomorphic, polymorphic, megamorphic, and blacklisted callsite state;
- observed scalar operand kinds;
- array layout and key-shape summaries;
- object class/layout/property-slot observations;
- branch bias;
- include/autoload target stability;
- guard-failure and blacklist summaries.

The parser rejects explicit userland value state: VM `Value`s, object handles,
array values, resource handles, non-interned request strings, and — as of the
writer-accounting slice — globals, superglobals, output buffers, and sessions.
Interned or engine-owned immutable strings are the only string payload class
accepted by the line-format validator.

## CLI Reporting

The VM CLI exposes the validation path without leaking data into PHP stdout:

```bash
php-vm run \
  --persistent-feedback-read target/performance/feedback/input.pff \
  --persistent-feedback-stats-json target/performance/feedback/stats.json \
  fixtures/runtime/valid/hello.php
```

The stats JSON (schema v3) records the resolved consumption policy
(`consume_mode`, plus `advisory_only` = consumption off), accepted entries,
entries written by the engine-owned writer, and rejection counts split by
cause — stale (source/engine/PHP-target/IR identity), epoch mismatch,
architecture mismatch, config mismatch, corrupt, and userland-state — plus
metadata bytes and whether execution fell back to baseline. Splitting the former
single `rejected_stale` counter lets an operator tell an out-of-date deployment
(config/arch/epoch) apart from a genuinely stale source.

Seeded execution is attributed in the VM counters JSON (`--counters-json`):
`persistent_feedback_seeded_sites` (installed at request start),
`persistent_feedback_seeded_guard_hits` (specialized executions that came from
a seed), and `persistent_feedback_seeded_dequickens` (seeds the guard protocol
rejected). These make a consumed-feedback run separately measurable from an
identical run with `--persistent-feedback-consume=off`.

## Matrix Policy

The acceleration matrix includes `persistent-feedback-advisory` only with
`--include-persistent-feedback` or
`PHRUST_ACCEL_MATRIX_PERSISTENT_FEEDBACK=1`.

The fastest-engine matrix includes `phrust-persistent-feedback-optional` only
with `--include-persistent-feedback` or
`PHRUST_FASTEST_MATRIX_PERSISTENT_FEEDBACK=1`.

Both rows are optional/default-off. They pin
`--persistent-feedback-consume=quickening`, exercise metadata validation, stats
reporting, and seeded execution, then compare PHP stdout, diagnostics, and exit
status against the baseline row.

## Writer Accounting (current slice)

`PersistentFeedbackContext::render_sites_counted` is the engine-owned writer: it
emits only validator-accepted entries and returns how many it wrote, which the
CLI records as `entries_written`. Emitted entries carry the executed run's
**final invalidation epochs** (class/function/autoload/include state stashed
out of `Vm::execute` and surfaced on `PhpExecutionOutput`), so entries record
their true observation state; a run that ends before teardown falls back to
conservative zeros. Writing is governed independently of consumption:
`--persistent-feedback-consume=off` still writes the sidecar, which is what
makes a seeded-vs-cold A/B run possible.

Epoch validation is explicit per context
(`PersistentFeedbackEpochValidation`): a live in-process consumer requires an
exact epoch match, while a cold-start load (the CLI reading a sidecar before
any code has executed) cannot know the epochs this run will reach — for a
matching source/config/IR fingerprint the declaration sequence replays
deterministically, so recorded epochs are kept on the accepted entries and
every consumer re-validates against live state at seed or lookup time.

## Consumption (current state)

Quickening seeding from accepted feedback is active whenever a feedback source
is loaded (the default sidecar next to the cached unit, an explicit
`--persistent-feedback-read`, or the server's persistent metadata store) *and*
the consume policy allows it. `php-vm run` consumes by default, matching the
default-on sidecar introduced with the default-on bytecode cache; the explicit
off switches are `--persistent-feedback-consume=off` and
`PHRUST_PERSISTENT_FEEDBACK_CONSUME=off` (and `PHRUST_PERSISTENT_FEEDBACK=off`
disables the sidecar wholesale). `--engine-preset=baseline` runs uncached and
never consumes.

`quickening_seed` flows to `QuickeningTable::seed_persistent_sites`, which
installs specialized/blacklisted sites already-warm but behind the **full guard
protocol**: a wrong seed self-corrects through dequickening and never changes
PHP-visible behavior. The installed count is recorded as
`persistent_feedback_seeded_sites`, and every guard hit or dequicken on a
seeded site is attributed via `persistent_feedback_seeded_guard_hits` /
`persistent_feedback_seeded_dequickens` — symmetric with the writer's
`entries_written`.

## Remaining Work

- widen the persisted payload beyond quickening sites and **monomorphic
  entry-unit function callsites** (which now persist as
  `site=ic_function_call` entries: callsite coordinates, lowered name, arity,
  observation epoch, and the IR-derived target function — see
  `FunctionCallSiteSnapshot` for the deliberately persistable subset).
  Method/property callsites and object-shape observations are blocked on
  replay-stable identity: their targets carry request-local class IDs and
  dynamic-unit indexes; scalar/array/branch observations already travel with
  the quickening sub-field where specializations exist;
- extend inline-cache seeding beyond monomorphic entry-unit function
  callsites (consumed today under `quickening-ics`, attributed via
  `persistent_feedback_seeded_callsites`/`_seeded_ic_hits`/
  `_seeded_ic_invalidations`) to method/property templates and later tiers —
  blocked on replay-stable class identity, as for payload persistence above;
- add Composer map fingerprints when the autoload graph model is promoted from
  request-local runtime behavior into persistent engine metadata.
