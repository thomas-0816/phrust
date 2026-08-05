Die entscheidende Trennung ist:

* Die **direkte Native-Value-/Array-/Reference-Plane** wird behalten. Sie hat ihr lokales Ziel erreicht: keine Rust-`Value`-Decode-/Encode-Arbeit mehr.
* Das darum gebaute **Publication-, Admission-, Tiering-, Fragment- und Call-System** muss weitgehend ersetzt werden. Genau dieses System verhindert, dass die direkte Plane überhaupt wirksam wird.

Das Ziel ist nicht „mehr Funktionen durch die heutige Optimizer-Zulassung bekommen“. Das Ziel ist:

> **Eine universelle, bereits schnelle Cranelift-Native-Engine für jeden PHP-Code, plus lokal optimierte Hot Regions. Keine schnelle und eine langsame Engine, keine Whole-Function-Zulassung und keine Publication-Vollständigkeitsbürokratie.**

## Das reale Zielbudget

PHP-FPM liegt in deinem Lauf bei 31,53 ms. Faktor 2 bedeutet für Phrust:

```text
p50 ≤ 63,1 ms
```

Daraus ergibt sich ungefähr dieses Budget:

```text
Request-Kompilation:             0 ms
HTTP, DB und unvermeidbare I/O: ähnlich PHP
Native PHP-Ausführung:          höchstens 35–45 ms
zusätzlicher Phrust-Overhead:    höchstens 20–30 ms
```

Eine Engine mit 3.997 Compiles, 39.968 Fragment-/Unit-Transitions und 574.019 Mikro-Helpern kann dieses Budget prinzipiell nicht erreichen.

---

# Was vom aktuellen Stand erhalten bleibt

Diese Arbeit ist nicht wertlos und soll nicht zurückgesetzt werden:

* Cranelift als einziger Executor;
* die direkte Native-Value-Repräsentation;
* direkte Array-, String-, Reference- und Object-Strukturen;
* genaue Native-Control-Results;
* Function-Indirection-Cells;
* persistenter Native-Code-Cache;
* Function-on-Demand-Identitäten;
* Frame-Arena;
* bestehende exakte Builtin-Handler;
* Callsite-, Klassen- und Property-Metadaten;
* Ownership- und SSA-Analysen.

Die direkten exakten Handler arbeiten bereits über `NativeRequestFastState` und `JitNativeControlResult`; sie sind brauchbare Bausteine.

---

# Was aus dem aktuellen Stand entfernt werden muss

## 1. Das gesamte „Total Publication before Optimizer Entry“-Modell

Der jetzige Compiler versucht, vor Eintritt in eine optimierte Funktion vollständig zu beweisen und zu reservieren:

* sämtliche Reference-Payloads;
* Array-Projektionen und -Mutationen;
* Output-Größen;
* String-Größen;
* Property-Zustände;
* Callback-Formen;
* Call-Frames;
* sämtliche möglichen Ergebnisse bestimmter Builtins.

Dafür existiert inzwischen ein sehr großer Satz aus `NativeEntryArrayRequirement`, Property-, String-, Resource-, Mutation-, Callback- und Output-Plänen.

Dieses Modell erzeugt:

```text
riesige Entry Guards
riesige CLIF-Funktionen
hohe Compile-Zeit
hohen Speicherbedarf
Whole-Function-Rejections
0 optimierte WordPress-Ausführung
```

Es muss weg.

### Neuer Vertrag

Ein Native-Wert ist bereits ein gültiger kanonischer Wert. Er darf Funktionen und Regionen durchlaufen, ohne vorher „total publiziert“ zu werden.

Optimierter Code prüft lokal:

```text
Tag
Shape/Layout
COW-Zustand
Referenzart
Call Target
```

Nur an der konkreten Operation, die diese Eigenschaft benötigt.

Nicht mehr:

```text
Kann diese komplette Funktion für alle denkbaren zukünftigen Werte
vollständig vorab publiziert werden?
```

---

## 2. Whole-Function-Optimizer-Admission vollständig abschaffen

Der derzeitige Compiler verwirft eine komplette optimierende Region unter anderem bei:

* By-Reference-Parametern;
* irgendeinem By-Reference-Callargument;
* dynamischen Callables;
* Includes und dynamischem Code;
* Fiber-Suspend;
* statischen Properties;
* relativen Klassenkonstanten;
* verschiedenen Builtin-Familien.

Das ist direkt im aktuellen `reject_unpublished_optimizer_boundaries()` implementiert.  Der entsprechende Commit bindet diese Prüfung unmittelbar in die Optimizer-Admission ein.

Diese Funktion und das zugrunde liegende Konzept müssen gelöscht werden.

## Zielmodell: regionenweise Optimierung

Eine Funktion kann so aussehen:

```text
direkte optimierte Region
    ↓
vorbereiteter By-Reference-Call
    ↓
direkte optimierte Region
    ↓
Include-/Autoload-Callout
    ↓
optimierte Schleife
    ↓
Return
```

Eine komplexe Operation beendet höchstens die aktuelle Hot Region. Sie disqualifiziert niemals die restliche Funktion.

PHPs eigener Tracing-JIT kompiliert heiße Code-Segmente und unterscheidet explizit zwischen Function- und Trace-JIT; der Trace-Modus profiliert laufend und kompiliert heiße Segmente, nicht nur vollständig homogene Funktionen. ([PHP][1])

V8 Maglev hängt Deoptimierungszustände an einzelne potenziell deoptimierende Nodes. Es verlangt nicht, dass eine komplette Funktion vor Eintritt für alle späteren Werte „total“ ist. ([V8][2])

Für Phrust bedeutet das:

```text
Hot Region miss
    → exakte Native-Core-Continuation

nicht:
Hot Function enthält irgendwo schwierige PHP-Semantik
    → ganze Funktion Helper-Baseline
```

---

# 3. Den heutigen Baseline-Tier durch einen schnellen „Native Core“ ersetzen

Das gegenwärtige Modell ist:

```text
Baseline:
    vollständige Semantik
    aber fast alles über kleine Runtime-Helper

Optimizing:
    direkt
    aber nur bei vollständig publizierbarer Funktion
```

Das kann niemals funktionieren.

Es muss ersetzt werden durch:

```text
Native Core:
    vollständige PHP-Semantik
    direkte elementare Operationen
    direkte Calls
    wenige grobkörnige Runtime-Callouts

Hot Region:
    dieselben Operationen
    plus SSA, Unboxing, Hoisting, Inlining und Spezialisierung
```

## Native Core ist nicht der heutige Baseline-Tier

Der Native Core muss bereits direkt ausführen:

* Locals und Temporaries;
* Scalar-Arithmetik und Vergleiche;
* Truthiness;
* Packed-/Record-Array-Zugriffe;
* Array-Iteration;
* deklarierte Property-Slots;
* COW-Fastpaths;
* Function- und Method-Calls;
* vorbereitete Builtins;
* Retain-/Release-Elision;
* direkte Returns.

Der Hot-Region-Compiler verbessert diesen Code anschließend, ist aber **nicht Voraussetzung für eine brauchbare Engine**.

### Zulässige grobe Runtime-Callouts

Der frühere Auftrag „keine Helper“ war zu pauschal. PHP selbst ruft für komplexe interne Operationen Runtime-/C-Funktionen auf.

Zulässig sind beispielsweise:

```text
execute_prepared_byref_call()
resolve_and_invoke_dynamic_callable()
compile_and_execute_eval()
execute_include_entry()
autoload_class()
preg_match_exact()
json_decode_exact()
invoke_property_hook()
```

Nicht zulässig sind:

```text
local_fetch()
local_store()
truthy()
compare()
array_fetch()
foreach_next()
property_fetch()
retain_copy()
```

Die Grenze lautet:

> **Ein Callout darf eine vollständige, tatsächlich komplexe PHP-Operation abbilden. Ein Callout darf keine elementare VM- oder Datenoperation ersetzen.**

Damit kann auch der universelle Native Core schnell sein, ohne sämtliche PHP-Semantik in CLIF nachzuprogrammieren.

---

# 4. Ein gemeinsames Native-Call- und Reference-ABI bauen

Die 14.761 durch By-Reference blockierten Call-Entscheidungen zeigen, dass References aktuell als Optimizer-Ausnahme statt als normaler PHP-Wert modelliert werden.

Das Call-ABI muss References normal tragen.

## Verbindliches Frame-Modell

Konzeptionell:

```rust
struct NativeFrame {
    slots: *mut NativeValue,
    arguments: *mut NativeArgument,
    return_slot: *mut NativeValue,
    caller: *mut NativeFrame,
    function: FunctionId,
    control: NativeControlState,
}

enum NativeArgument {
    Value(NativeValue),
    Reference(*mut NativeReferenceCell),
    LValue(NativeLValue),
}
```

`NativeLValue` beschreibt direkt:

```text
Local Slot
Array Element
Declared Property Slot
Static Property Slot
Reference Cell
```

## By-Reference-Call

Ein vorbereiteter Callsite-Plan enthält:

```text
Argument → Parameter Mapping
Reference/LValue-Ziel
Writeback-Ziel
Defaultwerte
Variadic-Bereich
Typprüfung
Return-by-reference Policy
```

Dann ist ein By-Reference-Call ein normaler Native Call:

```text
caller frame
    → direct callee cell
    → callee frame
    → writeback
```

Er darf weder:

* die Funktion aus dem Optimizer werfen;
* einen allgemeinen Call-Dispatcher betreten;
* eine neue Funktion kompilieren;
* den ganzen Callframe in Rust-Strukturen rekonstruieren.

## Function- und Method-Calls

* bekannte Funktionen: direkter Cell-Call;
* same-unit und cross-unit: identisches ABI;
* monomorphe Methoden: Class-ID-Guard plus Cell-Call;
* kleine polymorphe Sites: PIC;
* tatsächlich dynamische Callables: ein grober Resolver-Callout, danach PIC.

Die 39.968 Same-Unit-Transitions müssen zu echten Native Calls werden. Sie dürfen nicht mehr durch den VM-Koordinator oder eine Fragment-Continuation laufen.

## Builtins

Die vorhandenen exakten Handler werden auch vom Native Core verwendet, nicht ausschließlich von einem zugelassenen Optimizer.

Ein vorbereiteter Builtin-Call ist:

```text
exact handler address
exact native arguments
exact capability pointer
NativeControlResult
```

Keine Namenssuche, kein generischer Builtin-Executor, kein synthetischer IR-Call und kein allgemeiner Context-Aufbau.

Cranelift bietet direkte Calls und usergesteuertes Inlining; die Inlining-Entscheidung muss Phrust anhand des realen Callprofils treffen, weil Cranelift selbst keine vollständige Callgraph-/Hotness-Policy vorgibt. ([Wasmtime][3])

---

# 5. Die Fragmentarchitektur vollständig ersetzen

Der aktuelle Source schneidet Baseline-Regionen bereits nach 16 IR-Instruktionen und setzt sehr niedrige, stark konservativ geschätzte Fragmentgrenzen.

Das Resultat:

```text
432 IR-Instruktionen
40 Fragmente
11.899 CLIF-Blöcke
1,025 MB Code
2,48 s Compile-Zeit
```

ist kein Ausreißer. Es ist eine direkte Folge der Planung.

## Neue Regel: reale CFG zuerst, Split nur nach tatsächlichem Backend-Cost

Pipeline:

```text
PHP/Region CFG
    ↓
reale Extended Basic Blocks
    ↓
CLIF-Preflight
    ↓
nur bei tatsächlicher Überschreitung splitten
```

Cranelifts `FunctionBuilder` ist ausdrücklich für Extended Basic Blocks ausgelegt; der Frontend-Nutzer soll Kontrollfluss an tatsächlichen Branch-/Jump-Zielen aufteilen, nicht nach einer pauschalen Source-Instruktionszahl. ([Wasmtime][4])

## Fragmentgrenzen nur an

* Loop Headern;
* kalten Exception-/Error-Pfaden;
* groben Runtime-Callouts;
* sehr großen Live-Set-Grenzen;
* echten Backend-Größenlimits.

## Verbindliche Recovery-Gates

Für die genannte 432-Instruktions-Funktion:

```text
Native-Fragmente:        ≤ 3
CLIF-Blöcke:             ≤ 1.200
Codebytes:               ≤ 256–384 KB
Compile-Zeit:            ≤ 250 ms
rekursive Fragmenttiefe: 0
```

Für sehr große Funktionen wie `WP_Query::get_posts`:

```text
Native-Fragmente:        niedriger einstelliger Bereich
kein unbeschränkter Regalloc-Job
kein Instruktion-per-Fragment-Modell
```

## Konstante Stacktiefe

Fragmentwechsel erfolgen durch:

1. echten Native Tail Call, oder
2. einen einzigen konstanten Native-Fragment-Dispatcher.

Cranelift besitzt eine Tail-Calling-Convention für x86-64 und AArch64. ([Wasmtime][5])

Der Dispatcher wäre kein Interpreter:

```text
while next_fragment != RETURN:
    next_fragment = compiled_fragment(frame)
```

Er interpretiert keine PHP-Instruktionen. Er ruft ausschließlich bereits kompilierten Native Code auf.

Die Stacktiefe darf nicht von 39.968 Übergängen abhängen.

---

# 6. Request-synchrone Compilation vollständig abschaffen

3.997 Compiles in einem Request sind mit einem 63-ms-Ziel unvereinbar.

## Neues Deployment-Modell

Für Immutable Deployment:

```text
phrust build-image
    ↓
alle PHP-Dateien parsen
    ↓
jede Funktion einzeln in Native Core kompilieren
    ↓
Symbole, Klassen und Callsite-Pläne linken
    ↓
persistentes Deployment-Image
    ↓
Server lädt Image
    ↓
readiness
    ↓
Requests
```

Das ist kein Warm Runner:

* WordPress wird nicht ausgeführt;
* kein PHP-sichtbarer Request-State wird wiederverwendet;
* keine Datenbankantwort wird gecacht;
* es wird nur Quellcode kompiliert und gelinkt.

Jede Funktion wird weiterhin separat und bounded kompiliert. Es gibt keine Rückkehr zur früheren Whole-Unit-Compilation.

Im Request bleiben dynamisch:

* `eval`;
* tatsächlich neu erzeugter Code;
* veränderte Dateien im Development-Modus.

Für das immutable WordPress-Deployment gilt:

```text
compile attempts während jeder gemessenen Anfrage = 0
```

PHPs OPcache/JIT trennt ebenfalls zwischen Compile-on-load, Compile-on-first-execution, Profiling und Trace-Compilation; diese Policy ist eine Deployment-/Tiering-Entscheidung und nicht Teil jeder einzelnen Operation. ([PHP][1])

## Autoload

Composer- und WordPress-Autoload werden beim Deployment-Image symbolisch aufgelöst:

```text
class → source unit → native class/function entries
```

Autoload zur Laufzeit publiziert Symbole und führt den bereits kompilierten Top-Level-Entry aus. Es kompiliert nicht erneut.

Damit verschwindet der gemessene 14-Sekunden-Block aus Dynamic Code/Autoload.

---

# 7. Die Direct Data Plane behalten, aber Publication-Kopien eliminieren

Dein Profil zeigt:

```text
0 Rust-Value Decode
0 Rust-Value Encode
```

Die direkte Value Plane ist daher nicht der aktuelle Hauptfehler.

Aber:

```text
61 MB Direct-Array-Entries
14,7 MB Direct-Value-Slots
```

zeigen, dass Werte wahrscheinlich an Grenzen projiziert, kopiert oder erneut publiziert werden.

## Neuer Invariant

```text
Ein PHP-Array besitzt genau einen kanonischen Storage-Handle.
Ein String besitzt genau einen kanonischen Storage-Handle.
Eine Reference besitzt genau eine ReferenceCell.
Calls und Returns übertragen diese Handles.
```

Nicht:

```text
Caller Array
    → Entry Publication Array
    → Callee Array Projection
    → Return Publication Array
```

Ein Optimizer-Entry prüft nur:

```text
Tag
Shape ID
Mutation Epoch
COW Count
Reference Flag
```

Er kopiert nicht die Daten.

Das entfernt zugleich:

* große Entry-Publikationspläne;
* viele Entry-Guards;
* große direkte Arena-High-Water-Marken;
* Codegröße;
* Compile-Zeit.

Die aktuellen „total reference payload“-Änderungen bewegen sich noch in die entgegengesetzte Richtung: Fehlende oder uninitialisierte Pläne verlassen den Optimizer bereits vor Eintritt und werden in der Baseline nachpubliziert.  Dieses Modell muss beendet, nicht vervollständigt werden.

---

# 8. Der Optimizer wird ein Hot-Region-Compiler, kein Funktionszertifizierer

Nach dem Native-Core-Cutover:

```text
Native Core führt alles korrekt und direkt aus.
Profil sammelt:
    Branches
    Typen
    Shapes
    Call Targets
    Loop Counts
```

Dann werden kompiliert:

* heiße Loops;
* heiße Straight-Line-Regions;
* monomorphe Callketten;
* häufige Array-/Property-Pfade.

## OSR

```text
Native-Core Loop Header
    → Hot Region
```

## Side Exit

```text
Guard Failure
    → exakte Native-Core-Continuation
```

Der Zustand enthält nur live Values und Roots. Keine komplette Function-Publication.

Der Optimizer darf bei einem By-Reference-Call die Hot Region vor dem Call beenden und nach dem Call eine weitere Region beginnen. Er darf deshalb nicht die Funktion als Ganzes ablehnen.

## Inlining

Nach stabiler direkter Call-ABI:

* kleine Wrapper;
* Getter/Setter;
* häufige Predicates;
* kurze monomorphe Methoden;
* kleine pure Builtins.

Inlining wird nach:

```text
inclusive CPU saving / added code byte
```

entschieden, nicht nach Funktionsnamen.

---

# Die Zielarchitektur

```text
                         ┌─────────────────────────────┐
PHP IR ────────────────► │ Universal Native Core IR    │
                         │                              │
                         │ direct scalar/local ops      │
                         │ direct arrays/properties     │
                         │ canonical references         │
                         │ direct calls                 │
                         │ coarse semantic callouts     │
                         └──────────────┬───────────────┘
                                        │
                           ┌────────────▼─────────────┐
                           │ Native Core Cranelift    │
                           │ all PHP functions        │
                           │ already fast             │
                           └────────────┬─────────────┘
                                        │ profile/OSR
                           ┌────────────▼─────────────┐
                           │ Hot Region Cranelift     │
                           │ SSA, inlining, guards    │
                           │ unboxing, hoisting       │
                           └────────────┬─────────────┘
                                        │ side exit
                                        ▼
                           Native Core continuation
```

Nicht mehr:

```text
vollständig publizierbare Funktion?
    Ja  → Optimizer
    Nein → 574.000 Baseline-Helper
```

---

# Konkrete Umbauten am aktuellen Source

## Behalten

```text
native/direct value and array storage
exact native handlers
NativeControlResult
function cells
persistent cache
frame arena
ownership/value-flow analysis
Cranelift-only execution
```

## Löschen oder fundamental umschreiben

```text
reject_unpublished_optimizer_boundaries()
NativeEntry* total-publication requirements
whole-function optimizing admission
16-instruction Region splitting
per-local conservative CLIF-cost multiplication
Baseline-as-micro-helper-executor
operation-local publication/republication
request-synchronous immutable deployment compilation
recursive fragment-call chains
prepared builtin generic fallback
by-reference optimizer rejection
```

## Neu bauen

```text
Universal Native Core lowering
canonical NativeFrame/NativeArgument/LValue ABI
coarse typed semantic callout ABI
real-CFG / actual-cost fragment planner
constant-stack fragment transition
offline deployment-image compiler
hot-region profiling + OSR
profile-directed inlining
```

---

# Nicht verhandelbare Gates

## Native Core Gate

Ohne Hot-Region-Optimizer:

```text
HTTP 200
korrekter Body
keine Anfrage > 2 s
keine Request-Compilation
kein Stack Overflow
keine Whole-Function-Rejection
p50 ≤ 120 ms
```

Das ist wichtig: Der Optimizer darf nicht erneut die einzige Chance auf brauchbare Geschwindigkeit sein.

## Struktur-Gate

```text
by-reference whole-function rejects:       0
dynamic-code whole-function rejects:       0
same-unit VM/transition calls:              0
request compile attempts:                   0
fragment stack growth:                      0
operation-local micro-helper fallbacks:     0
```

## Hot-Region-Gate

```text
Anteil heißer CPU-Zeit in Hot Regions:      ≥ 80–90 %
repeated side exits an stabilen Sites:       0
hot loops mit OSR:                           vollständig
```

## Faktor-2-Gate

```text
Phrust warm c1 p50:                          ≤ 63,1 ms
p95:                                         ≤ 2x PHP-p95
HTTP/body correctness:                       identisch
```

---

# Warum diese Architektur den erforderlichen Sprung adressiert

Aus deinem Profil entfallen exklusiv:

```text
37,79 s  Builtins
29,33 s  Function Calls
14,03 s  Dynamic Code / Autoload
 9,78 s  Method Calls
 5,22 s  Reference Binding
```

Diese fünf Familien erklären zusammen fast die gesamte gemessene Helper-Zeit.

Der vorgeschlagene Umbau greift sie direkt an:

| Aktueller Verlust     | Zielumbau                                     |
| --------------------- | --------------------------------------------- |
| Builtin-Dispatcher    | exakte Handler in Native Core und Hot Regions |
| Function-Helper       | direkte NativeFrame-/Cell-Calls               |
| Dynamic Compilation   | Offline Deployment Image                      |
| Method-Helper         | Class Guard + Method Cell/PIC                 |
| Reference-Rejects     | kanonisches LValue-/Reference-Call-ABI        |
| 39.968 Transitions    | echte Calls und wenige reale Fragmente        |
| 100 % Helper-Baseline | schneller universeller Native Core            |
| 0 % Optimizer         | Hot Regions ohne Whole-Function-Zulassung     |

Das ist kein erneuter Versuch, mehr Publication-Pläne zu ergänzen. **Es entfernt das Publication-Modell als Voraussetzung für Geschwindigkeit.**

Eine Garantie für exakt Faktor 2 wäre vor der Umsetzung unseriös. Aber ich würde keine andere Architektur mehr als glaubwürdigen Weg akzeptieren. Der aktuelle Code kann durch weitere Publication-Fixes nicht auf Faktor 2 kommen. Ein schneller universeller Native Core mit regionaler Optimierung, direktem Call-/Reference-ABI, realer CFG und vorkompiliertem Deployment kann es.

[1]: https://www.php.net/manual/en/opcache.configuration.php?utm_source=chatgpt.com "PHP: Runtime Configuration - Manual"
[2]: https://v8.dev/blog/maglev?utm_source=chatgpt.com "Maglev - V8’s Fastest Optimizing JIT · V8"
[3]: https://docs.wasmtime.dev/api/cranelift_codegen/inline/index.html?utm_source=chatgpt.com "cranelift_codegen::inline - Rust"
[4]: https://docs.wasmtime.dev/api/cranelift/prelude/struct.FunctionBuilder.html?utm_source=chatgpt.com "FunctionBuilder in cranelift::prelude - Rust"
[5]: https://docs.wasmtime.dev/api/cranelift/prelude/isa/enum.CallConv.html?utm_source=chatgpt.com "CallConv in cranelift::prelude::isa - Rust"

