# Cranelift für maximale Execution Speed auf eigenem Bytecode

Cranelift ist als schneller Compiler mit gutem — nicht maximalem — Codeniveau ausgelegt. Um das Maximum an Laufzeitperformance herauszuholen, müssen drei Ebenen zusammenspielen: die richtigen Compiler-Settings, ISA-Flags passend zur Ziel-CPU, und vor allem ein Frontend, das CLIF so erzeugt, dass die Optimierungspassen überhaupt greifen können. Cranelift optimiert nur, was die IR ihm semantisch erlaubt.

---

## 1. Compiler-Settings

Die shared settings steuern den gesamten Mid-End. Ohne `opt_level=speed` läuft der aegraph-Optimizer (GVN, LICM, Constant Folding, Rewrite Rules, Redundant Load Elimination) gar nicht.

```rust
use cranelift_codegen::settings::{self, Configurable};

let mut flags = settings::builder();

// Zentraler Schalter: aktiviert den kompletten aegraph-Mid-End.
// "none" überspringt ihn vollständig.
flags.set("opt_level", "speed").unwrap();

// Alias-Analyse + Redundant Load Elimination / Store-to-Load-Forwarding
flags.set("enable_alias_analysis", "true").unwrap();

// Verifier: während der Entwicklung AN (fängt Fehler in handgebautem CLIF),
// in Produktion AUS (kostet nur Compile-Zeit)
flags.set("enable_verifier", "false").unwrap();

// Frame Pointer als zusätzliches Register freigeben —
// nur wenn kein Unwinding/Sampling-Profiling über Frame Pointers benötigt wird
flags.set("preserve_frame_pointers", "false").unwrap();

// NaN-Kanonisierung nur bei Determinismus-Anforderungen (kostet FP-Performance)
flags.set("enable_nan_canonicalization", "false").unwrap();
```

`opt_level=speed_and_size` ist die Alternative, wenn Code-Größe (Instruction-Cache-Druck) eine Rolle spielt — bei großen generierten Codemengen kann das paradoxerweise schneller sein.

---

## 2. ISA-Flags: CPU-Features freischalten

Die Instruction Selection (ISLE-Patterns) kann bessere Encodings nur wählen, wenn die entsprechenden Features deklariert sind. Ohne sie fällt der Backend auf Baseline-Instruktionen zurück.

**JIT (Ziel = Host-CPU):**

```rust
// Erkennt has_avx, has_avx2, has_bmi1, has_bmi2, has_lzcnt,
// has_popcnt, has_fma usw. automatisch
let isa = cranelift_native::builder().unwrap()
    .finish(settings::Flags::new(flags)).unwrap();
```

**AOT (fremde Zielhardware):** Features manuell und konservativ setzen — nur das garantierte Minimum der Zielplattform deklarieren:

```rust
let mut isa_builder = cranelift_codegen::isa::lookup_by_name("x86_64").unwrap();
isa_builder.set("has_avx2", "true").unwrap();
isa_builder.set("has_bmi2", "true").unwrap();
// ...
```

---

## 3. CLIF-Erzeugung: der größte Hebel

### 3.1 MemFlags — Loads für den Optimizer freischalten

Der wichtigste Einzelhebel. Ein Load ist für Cranelift per Default side-effecting (kann trappen) und damit für GVN, LICM und RLE tabu. Erst die Annotationen des Frontends machen ihn „pure":

```rust
use cranelift_codegen::ir::MemFlags;

let mut mf = MemFlags::new();
mf.set_notrap();     // Zugriff kann nicht trappen → GVN/LICM/RLE dürfen zugreifen
mf.set_aligned();    // erlaubt bessere/kompaktere Encodings
mf.set_readonly();   // Wert ändert sich nie → aggressives Hoisting aus Schleifen

builder.ins().load(types::I64, mf, addr, offset);
```

Praktische Regel: Jeder Speicherbereich des eigenen Runtime-Modells, für den das Frontend Nicht-Trappen, Alignment oder Unveränderlichkeit garantieren kann (Konstantenpool, Typ-Metadaten, VTables, interne Strukturen mit bekanntem Layout), wird annotiert. Der Unterschied ist konkret: ein nicht annotierter Load bleibt in der heißen Schleife, ein annotierter wird einmal davor ausgeführt.

### 3.2 Werte in SSA halten, nicht in Stack-Slots

Cranelift hat kein vollwertiges mem2reg/SROA. Lokale Variablen des eigenen Bytecodes, die als Stack-Slot mit Load/Store abgebildet werden, sind für den Mid-End weitgehend opak. Stattdessen `cranelift-frontend` mit `Variable` nutzen — der `FunctionBuilder` erledigt die SSA-Konstruktion inklusive Block-Parametern an Merge-Points:

```rust
use cranelift_frontend::{FunctionBuilder, Variable};

let var = builder.declare_var(types::I64);
builder.def_var(var, value);
let v = builder.use_var(var);   // liefert den SSA-Wert, kein Memory-Traffic
```

Stack-Slots nur für Werte, deren Adresse tatsächlich genommen wird oder die zu groß für Register sind.

### 3.3 Control-Flow-Layout steuern

Cranelift baut im Mid-End keinen Control-Flow um — das Layout, das das Frontend liefert, ist das Layout, das gilt:

```rust
// Slow-Paths (Fehlerbehandlung, Deopt, seltene Fälle) aus dem Hot-Path räumen
builder.set_cold_block(error_block);
```

Bedingte Sprünge (`brif`) so orientieren, dass der wahrscheinliche Zweig der Fallthrough ist. Bei Dispatch-Strukturen des eigenen Bytecodes (z. B. Opcode-Switch) `br_table` statt Vergleichsketten verwenden.

### 3.4 Typen und Operationen präzise wählen

Native Registerbreiten (I64 auf 64-bit-Targets) bevorzugen und unnötige Extend/Truncate-Ketten vermeiden. Wo das Sprachmodell es erlaubt, Integer-Semantik ohne Overflow-Traps verwenden — geprüfte Arithmetik erzeugt zusätzliche Branches, die der Optimizer nicht entfernen kann.

---

## 4. Inlining

Der Cranelift-Inliner (seit Wasmtime 36 in der Codebasis, noch in Reifung) ist embedder-gesteuert: Cranelift stellt die Mechanik, der Embedder entscheidet per Callback, welcher Call ersetzt wird. Bei eigenem Bytecode gibt es zwei Wege:

1. **Cranelift-Inliner-API** mit eigener Heuristik (Größe des Callees, Aufruf-Häufigkeit, Schleifentiefe des Call-Sites) füttern.
2. **Inlining auf Ebene des eigenen Bytecodes/AST**, bevor CLIF erzeugt wird — meist der stärkere Weg, weil dort semantisches Wissen verfügbar ist (Devirtualisierung, bekannte Konstanten, Spezialisierung).

In beiden Fällen gilt: Inlining ist die Optimierung mit dem größten indirekten Effekt, weil sie GVN, Constant Folding und RLE im Kontext des Call-Sites erst ermöglicht.

---

## 5. Was das Frontend selbst leisten muss

Cranelift führt bewusst nicht aus: Loop Unrolling und andere Loop-Transformationen, Autovektorisierung, Control-Flow-Umbau, interprozedurale Analysen jenseits des Inliners, Devirtualisierung. Alles, was Struktur- oder Semantikwissen über den eigenen Bytecode erfordert, gehört in einen Frontend-Pass vor der CLIF-Erzeugung:

- Loop Unrolling / Peeling für bekannte heiße Schleifen
- Explizite SIMD-Emission (`I32X4`, `F64X2`, …) — die ISA-Flags sorgen dann für gute Encodings; automatisch vektorisiert wird nichts
- Devirtualisierung / Guarded Inlining von Indirect Calls des Dispatch-Modells
- Konstanten-Spezialisierung von Funktionen (Cloning mit eingesetzten Konstanten)

Cranelift übernimmt danach zuverlässig: skalare Rewrites, GVN, LICM, RLE, Dead-Code-Elimination via Extraktion, Register Allocation (regalloc2) und Instruction Selection.

---

## 6. Modul-Wahl und Compile-Pfad

- **JIT:** `cranelift-jit` (`JITModule`) — kompiliert in ausführbaren Speicher, Symbolauflösung zur Laufzeit.
- **AOT:** `cranelift-object` — erzeugt Objektdateien für normales Linken.

Der Mid-End läuft automatisch in `Context::compile()`; wer Zwischenstände inspizieren will, ruft explizit `ctx.optimize(isa)` auf und lässt sich das optimierte CLIF ausgeben — nützlich, um zu prüfen, ob Hoisting und GVN tatsächlich greifen.

---

## 7. Checkliste

| Maßnahme | Wirkung |
|---|---|
| `opt_level=speed` | Aktiviert den gesamten aegraph-Mid-End |
| `enable_alias_analysis=true` | RLE und Store-to-Load-Forwarding |
| ISA-Flags via `cranelift_native` (JIT) | Bessere Instruction Selection (AVX2, BMI, FMA, …) |
| `enable_verifier=false` (nur Produktion) | Schnellere Kompilierung |
| `preserve_frame_pointers=false` | Ein Register mehr für den Allocator |
| `MemFlags` (`notrap`/`readonly`/`aligned`) | Schaltet GVN/LICM/RLE für Loads frei — größter Einzelhebel |
| SSA via `Variable` statt Stack-Slots | Mid-End sieht Werte statt opaken Speicherverkehr |
| `set_cold_block` + Fallthrough-Orientierung | Besseres Code-Layout, I-Cache-Nutzung |
| Inlining (API oder Frontend-Ebene) | Schaltet nachgelagerte Optimierungen frei |
| Loop-Opts/SIMD/Devirtualisierung im Frontend | Kompensiert, was Cranelift bewusst nicht tut |

## 8. Validierung

Jede dieser Maßnahmen mit realen Workloads benchmarken — insbesondere den noch reifenden Inliner und die `speed` vs. `speed_and_size`-Entscheidung. Das optimierte CLIF (`ctx.optimize` + Ausgabe) und der erzeugte Maschinencode (Disassembly) sind die verlässlichen Kontrollpunkte, ob Annotationen und IR-Struktur die erwarteten Optimierungen tatsächlich auslösen.


