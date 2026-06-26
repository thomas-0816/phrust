---
name: phpt-module
description: Run the phrust PHPT module loop for a given MODULE (e.g. /phpt-module standard.strings). Baseline from the committed manifests, triage failures by owning layer, fix in that crate, verify with the narrowest focused run, then the module gate, and the full regression only at acceptance. Use when working a PHPT module end to end.
disable-model-invocation: true
---

# phrust PHPT module loop

Work the module named in the argument (e.g. `standard.strings`). Everything runs
through the Nix dev shell (`nix develop -c …`). The reference oracle is PHP
**8.5.7** at `third_party/php-src/sapi/cli/php`; build it first if missing
(`just bootstrap-ref` then `just build-ref-php`).

## Discipline: run only the tests you need

This is the core rule. During iteration run the **narrowest** selector, not the
module or the corpus:

1. Build once per change set: `nix develop -c just phpt-dev-build`
2. One test: `nix develop -c just phpt-fast MODULE=<m> FILE=<one.phpt>`
   A family: `nix develop -c just phpt-fast MODULE=<m> PATTERN=<glob*>`
3. Re-check only what was failing: `nix develop -c just phpt-rerun-failures MODULE=<m>`

Reserve the broad gates for checkpoints / acceptance, NOT the per-edit loop:
- module checkpoint: `nix develop -c just phpt-dev-module MODULE=<m>`
- domain gate before handoff: `just verify-frontend` (literals) / `just verify-runtime` (builtins)
- acceptance only: `PHPT_RUN_FULL=1 nix develop -c just phpt-full-regression`

## Loop

1. **Baseline (source of truth).** Read module status from the committed
   manifests, not the rendered `docs/phpt/modules/<m>.md`:
   `tests/phpt/manifests/full-baseline-module-counts.jsonl`,
   `tests/phpt/manifests/modules/<m>.selected.jsonl`. (`just phpt-triage`
   re-projects these from the baseline.)
2. **Triage.** Bucket failures by owning layer (consider the `phpt-failure-triage`
   subagent). Fix frontend/source-decoding gaps before runtime/builtin gaps.
3. **Fix in the owning crate only** (lexer/syntax/ast/semantics/ir/runtime/std/vm).
   Do not solve a runtime bug inside PHPT tooling. Use arginfo for builtin
   signatures; do not hand-write them.
4. **Prove it** with the narrowest focused run (step 2 above), oracle-checked.
5. **Add a regression fixture** for every behavior change — a minimized generated
   PHPT with provenance under `tests/phpt/generated/<m>/` (use the
   `new-phpt-fixture` skill).
6. **Checkpoint** with the module + domain gate once a cluster is closed.
7. **Acceptance:** run the full regression; it must report **no new rejected
   regression fingerprints**. `PHPT_ACCEPT_BASELINE=1` only with an explicit,
   written justification of exactly which new fingerprints are accepted.

## Hard rules

- Never edit `third_party/php-src/` (read-only oracle + original PHPTs).
- Never commit `target/` artifacts.
- Never hand-edit baseline manifests or rendered module docs — regenerate them.
- Every known gap needs: ID, reference behavior, current behavior, example
  fixture, owning layer, baseline count.

## Report at the end

- Module PASS/SKIP/FAIL/BORK before → after (from the manifests).
- Frontend fixes vs runtime/builtin fixes made.
- Resolved fingerprints and any remaining documented gaps.
- Confirmation the full regression produced no new rejected fingerprints.
