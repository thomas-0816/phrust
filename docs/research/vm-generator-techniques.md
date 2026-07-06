# VM-Generator and Meta-Compiler Techniques (research only)

Date: 2026-07-06.

This is a **research note**, not a plan. It evaluates VM-generator, generated-
interpreter, copy-and-patch, and meta-compiler techniques as input to phrust's
existing performance stack. It exists to prevent a tempting-but-wrong rewrite.

**Do-not-start conclusion:** phrust already owns a mature lexer → parser/CST →
typed AST → semantic frontend/HIR → IR → runtime → VM pipeline with dense
bytecode, quickening, inline caches, superinstructions, an opcache, a
feature-gated Cranelift tier, and report-only copy-and-patch/mid-tier/deopt
research. A VM generator or meta-compiler rewrite would risk PHP-visible
compatibility, duplicate this stack, and introduce a second semantic execution
path — the one thing the architecture forbids. The useful takeaways are
**metadata-generation and tooling** improvements that reduce hand-maintenance,
not a new engine.

## External techniques → phrust-owned equivalents

| External technique | Idea | Phrust-owned equivalent (today) | Incremental, non-rewrite direction |
| --- | --- | --- | --- |
| VM generator (e.g. generated interpreters) | Generate dispatch loop + handler bodies from a spec | Hand-written dense dispatch in `php_vm::bytecode` / `php_vm::vm`; opcode metadata in `DenseOpcode` | Generate *handler metadata tables* (opcode → family/operands/side-effect class) from a single source, not the handler bodies |
| Generated interpreter tables | Derive decode/dispatch tables from an ISA description | `DenseOpcode`/`DenseOperands` enums + `as_str`/family maps kept in sync by hand | A `build.rs`/macro that derives `as_str`, family classification, and verifier operand-shape checks from one opcode definition |
| Copy-and-patch stencils | Emit machine code by concatenating pre-compiled stencils + patching | Report-only stencil model in `docs/research/copy-and-patch-stencil-tier.md` (`dump-copy-patch-stencils`) | Keep report-only; generate the stencil *catalog* (patch sites, helper ABI, live-state) from the same opcode metadata rather than a parallel hand-list |
| Superinstruction mining | Discover hot opcode pairs, fuse them | `php-vm dump-bytecode-patterns` + `scripts/performance/superinstruction_patterns.py` + selected fusion set (`docs/performance/superinstructions.md`) | Already generator-shaped; extend the miner, keep exact fallback accounting |
| Handler DSL | Describe handlers in a DSL, generate Rust | None; handlers are direct Rust | Not worth it — a DSL would obscure PHP-visible semantics and add a translation layer. Prefer shared helper functions |
| IC schema generation | Generate inline-cache record layouts + guards | Hand-written function/method/property/builtin/include ICs (`php_vm::inline_cache`) + `docs/performance/quickening-inline-caches.md` | Generate IC *guard/epoch field schemas* + their JSON/report rendering from one definition to cut drift |
| Counter/metric generation | Generate counter structs + serializers | `php_vm::counters` (hand-written struct + `to_json` + record methods) + `docs/performance/counter-families.md` | A macro to derive the counter field ↔ JSON key ↔ record-method triple would remove a real, recurring maintenance cost (each new counter touches 3+ sites) |
| Meta-compiler / PHP-aware mid-tier | Compile a higher IR to native with semantic guards | Report-only mid-tier plan (`docs/research/php-mid-tier-compiler.md`, `dump-mid-tier-plan`) | Feed it better property/method/alias metadata (already in progress); do not make it executable prematurely |
| Deopt/OSR table generation | Generate resume tables from liveness analysis | Report-only `php_vm::deopt` metadata + verifier | Improve metadata precision + generate the reason-code ↔ label ↔ Cranelift-side-exit mapping from one source |

## Small tooling improvements that reduce duplication (non-rewrite)

These reduce hand-maintenance without a second execution path. Each is
independently landable and does not block current fastest-engine work:

1. **Opcode metadata single-source.** Derive `DenseOpcode::as_str`, family
   classification, and verifier operand-shape expectations from one opcode
   definition (macro or `build.rs`). Today these are kept in sync by hand across
   the lowering, verifier, renderer, and report classifiers — a recurring drift
   surface (every new opcode touches all of them).
2. **Counter triple derivation.** A derive macro for the
   `VmCounters` field ↔ JSON key ↔ `record_*` method triple. New counters
   currently touch the struct, `Default`, `to_json`, a record method, and often
   a classification test.
3. **Report classifier reuse.** The copy-patch stencil, mid-tier plan, and
   baseline-native stencil reports each classify opcodes independently. A shared
   opcode-metadata table (from #1) would let all three consume one classification
   with report-specific overlays.

## Hard constraints (why this stays research)

- **No interpreter rewrite.** The dense + rich interpreter is the single
  semantic source of truth.
- **No new bytecode format.** Dense bytecode is versioned and shared by the
  opcache, quickening, superinstructions, and all report tiers.
- **No second semantic execution path.** Any generated artifact must be
  metadata/tooling that feeds the existing path, never a parallel evaluator.
- **Does not block current work.** Superinstruction mining and report metadata
  generation already exist; the tooling improvements above are opportunistic.

## References

- `docs/performance/superinstructions.md`
- `docs/performance/quickening-inline-caches.md`
- `docs/performance/counter-families.md`
- `docs/research/copy-and-patch-stencil-tier.md`
- `docs/research/php-mid-tier-compiler.md`
- `docs/performance/deopt-live-state-osr-metadata.md`
- `docs/performance/fastest-engine-known-gaps.md`
