## Klare Diagnose

Das bisherige Konzept war architektonisch falsch. Es hat Cranelift nicht zur eigentlichen PHP-Ausführungsmaschine gemacht, sondern zwei problematische Ebenen gebaut:

1. einen **Closed-World-Optimizer**, der nur Funktionen akzeptiert, für die praktisch sämtliche Werte, Referenzen, Ressourcen, Call-Ziele und Seiteneffekte bereits vollständig „publiziert“ und vorab bewiesen sind;
2. eine formal native, tatsächlich aber **aseline**, die bei gewöhnlichen PHP-Sprachmerkmalen die Kontrolle permanent aus dem erzeugten Code herausführt.

Dieser Weg darf nicht weiter optimiert werden. Weitere Publication-Proofs, mehr Fragmentgrenzen, höhere Limits oder schnellere Einzel-Helper würden den grundlegenden Fehler nur konservieren.

## Die notwendige Architekturentscheidung

Phrust braucht ab jetzt genau zwei Ausführungsebenen:

> **Einen garantiert vollständigen Generic-Cranelift-Tier und einen spekulativ optimierenden Cranelift-Tier. Der Optimizer fällt lokal auf generische native Operationen zurück, niemals auf einen Rust-Executor oder eine andere PHP-Ausführungsroute.**

Damit gilt:

* Jede PHP-Funktion hat immer einen ausführbaren Cranelift-Body.
* Jede heiße PHP-Funktion bekommt zusätzlich einen optimierten Cranelift-Body.
* By-Reference, dynamische Calls, Includes, Methoden, Closures, Exceptions oder variable Argumente dürfen niemals mehr die Kompilierung einer ganzen Funktion verhindern.
* Runtime-Helper bleiben für atomare Laufzeitdienste erlaubt, etwa Hash-Tabellen, PCRE, MySQL oder komplexe Builtins. Sie dürfen aber keine PHP-Funktion ausführen, keinen kompletten Call dispatchen und keinen PHP-Kontrollfluss interpretieren.

Das ist der entscheidende Unterschied zwischen „mit Cranelift erzeugter Stub“ und einer echten Cranelift-Ausführung.

---

# Warum der jetzige Optimizer WordPress grundsätzlich nicht erreichen kann

## 1. Ein einzelnes gewöhnliches PHP-Feature verwirft die ganze Region

Der aktuelle Optimizer lehnt eine komplette Region unter anderem ab, sobald sie Folgendes enthält:

* By-Reference-Parameter,
* By-Reference-Call-Argumente,
* dynamische Callables,
* Includes oder anderen Dynamic Code,
* Fiber-Suspend,
* statische Properties,
* relative Class Constants,
* bestimmte ressourcenabhängige Builtins.

Das ist nicht bloß eine noch unvollständige Feature-Matrix. Es ist die falsche Granularität: Eine lokal schwierige Operation vernichtet die Optimierung der gesamten Funktion.

Zusätzlich enthält `NativeOptimizingAdmission` eine extrem umfangreiche Closed-World-Beschreibung mit „total“-Markierungen, exakten Array-Anforderungen, Property-Plänen, Objektlayouts, Ressourcenklassen und vorab berechneten Value-, Array- und String-Allokationsbudgets. Selbst Builtins gelten nur dann als total, wenn Argumentklassen, Ownership, Cleanup, alle Ergebniszustände und mehrere nichtleere Ressourcenbudgets vorab feststehen.

Für WordPress ist diese Bedingung strukturell unerfüllbar. WordPress ist gerade ein Programm mit dynamischer Verlinkung, Autoloading, Callables, Hooks, Referenzen und wechselnden Array-Formen.

## 2. Die Baseline erklärt fast alle normalen PHP-Calls zu dynamischen Fällen

Der aktuelle Call-Pfad betrachtet unter anderem diese Formen als Grund für den generischen Dispatcher:

* named und unpacked arguments,
* By-Reference,
* Methoden und statische Methoden,
* Cross-Unit-Calls,
* typed parameters,
* variadische Funktionen,
* Defaults und zusätzliche Argumente,
* Closures und Captures,
* Funktionen mit Try/Throw-Metadaten.

Das ist nahezu die Definition eines realen PHP-Frameworks.

Die von dir gemessenen 14.761 By-Reference-Rejections sind deshalb kein Sonderfall, der noch einen zusätzlichen Spezialhandler benötigt. Sie zeigen, dass die derzeitige Direkt-Call-Definition für PHP ungeeignet ist.

## 3. Selbst „direkte“ Builtins verlassen den residenten Cranelift-Pfad

`jit_baseline_native_builtin_dispatch_impl` wechselt in den kalten Rust-Kontext, sucht oder lädt Prepared-Builtin-Metadaten, baut PHP-Control-Zustände auf und ruft dort die Baseline-Builtin-Ausführung auf. Der generische Call-Dispatcher rekonstruiert zusätzlich Callsite-Descriptor, Argumentlisten, lokale Slots und Ergebniszustände.

Dass der Rust-`Value`-Decode/Encode-Zähler null ist, widerspricht dem nicht. Die direkte Value-Ebene funktioniert offenbar. Das Problem ist die **Kontrollübergabe an umfangreiche Rust-Dispatcher auf fast jeder semantischen Grenze**.

## 4. Das Tiering steckt in einem Bootstrap-Deadlock

Der Server führt bei einer Tiering-Entscheidung zunächst erneut die Baseline aus. Erst wenn deren Ergebnis erfolgreich ist, wird die Hintergrundoptimierung eingeplant. Ein Request mit HTTP 500 erzeugt daher absichtlich keinen Optimizer-Job.

Hinzu kommen derzeit:

* genau ein Optimizer-Worker,
* eine Queue-Kapazität von genau einem Job,
* Verwerfen weiterer Kandidaten, sobald diese Queue voll ist.

Damit könnte selbst ein funktionierender Request die mehrere Tausend Funktionen umfassende Working-Set-Kompilierung nur extrem langsam aufbauen.

## 5. Dynamische Verlinkung ist fälschlich Teil der Maschinen-Code-Identität

Der Native-Cache-Key berücksichtigt komplette externe Signaturen einschließlich:

* Parametername,
* By-Reference,
* Variadic,
* nativer Arity,
* Return-by-Reference,
* Exception-Routen,
* nativer Parameterform,
* Defaults,
* Methodenspezialisierungen und Receiver-Layout.

Damit erzeugt eine dynamische PHP-Anwendung Varianten von Maschinen-Code, obwohl die dynamische Verlinkung in writable Entry- und Inline-Cache-Tabellen gehören müsste.

Das ist eine wesentliche Erklärung für die 3.997 Kompilierungen innerhalb eines Requests.

## 6. Fragmentierung behandelt nur das Symptom der Codeexplosion

Der Planner setzt bereits für Baseline und Optimizer eine maximale Chunk-Größe von 16 Region-Instruktionen. Der Kommentar im Quelltext hält ausdrücklich fest, dass eine 31-Instruktionen-WordPress-Sequenz um ungefähr zwei Größenordnungen in CLIF-Blöcke expandiert. Calls, Locals, Arrays und Cleanup werden deshalb mit vielen zusätzlichen Kontrollblöcken kalkuliert.

Dein Einzelbeispiel ergibt:

* 27,54 CLIF-Blöcke pro Region-IR-Instruktion,
* 2.373 Bytes Maschinen-Code pro Region-IR-Instruktion,
* 40 Fragmente aus einer Funktion mit nur 432 Region-Instruktionen.

Das ist kein Problem, das durch einen noch genaueren Fragment-Estimator behoben wird. Der Lowerer dupliziert viel zu viel Guard-, Ownership-, Cleanup-, Side-Exit- und Continuation-Logik.

---

# Was vom jetzigen Stand erhalten bleiben sollte

Es wäre ebenfalls falsch, alles wegzuwerfen. Der Quelltext enthält bereits wesentliche Teile der richtigen Infrastruktur:

* die direkte Value-, Array- und String-Ebene;
* bevorzugte und Baseline-Entry-Cells;
* `JitNativeLinkedFunction` für Cross-Unit-Verlinkung;
* ein einheitliches `JitNativeCallFrame`;
* `JitNativeCallArgument` mit `BY_REFERENCE`, `source_slot` und `property_receiver`;
* generation-safe Indirection Entries;
* direkte Runtime-Views.

Diese Strukturen zeigen, dass der vollständige Neubau einer Value-Repräsentation nicht erforderlich ist.

**Erhalten werden sollten also:**

* die direkte Handle-/Slot-Repräsentation;
* die nativen Arenen;
* die Entry-Cells;
* die vorhandenen Lvalue-Metadaten;
* die persistenten Artifact-Grundlagen;
* die IR- und Cranelift-Infrastruktur.

**Ersetzt werden müssen:**

* die all-or-nothing Optimizer-Admission;
* der generische Rust-Call-Executor;
* der Builtin-Baseline-Dispatcher;
* Dynamic-Code-„compile and invoke“-Helper;
* Signatur- und Layout-abhängige Maschinen-Code-Keys;
* die 16-Instruktionen-Fragmentierungsstrategie;
* das Tiering erst nach erfolgreichem Request.

---

# Zielarchitektur

```text
PHP source
   │
   ▼
Immutable PHP IR / Unit Image
   │
   ├── Generic Cranelift body       immer vorhanden, semantisch vollständig
   │
   └── Optimized Cranelift body     für jede heiße Funktion
          │
          ▼
Preferred Entry Cell  ◄──────────── atomare Publication
          │
Generated call / call_indirect
          │
          ▼
Generated callee body

Cold resolver:
  Symbolauflösung, Autoload, Include-Load, IC-Miss, Artifact-Load
  └── liefert Entry Cell + Binding Plan zurück
      └── führt den PHP-Callee niemals selbst aus

Runtime leaves:
  Arrays, Strings, Objekte, Builtins, Extensions, I/O
  └── atomare Operationen, kein PHP-Call-Dispatcher
```

Die wichtigste Invariante lautet:

> **Nach der Auflösung eines Calls führt generierter Code den generierten Callee aus. Rust darf auflösen und binden, aber nicht den Userland-Call übernehmen.**

---

# Die sechs notwendigen Umbauten

## 1. Aus der Baseline muss ein vollständiger Generic-Cranelift-Tier werden

Der bisherige Baseline-Tier ist nicht als Produktionsfallback geeignet. Er muss durch einen semantisch vollständigen, kompakten Cranelift-Tier ersetzt werden.

Dieser Tier:

* kompiliert den vollständigen CFG einer PHP-Funktion;
* verwendet zunächst die bestehende generische `i64`-/Handle-Repräsentation;
* hält Locals in nativen Slots;
* führt Branches, Loops, lokale Loads/Stores und Calls in generiertem Code aus;
* ruft für komplexe Einzeloperationen Leaf-Runtime-Funktionen auf;
* unterstützt jede IR-Instruktion;
* kennt keinen „unsupported shape“-Rückfall auf einen anderen Executor.

Der bisherige Baseline-Dispatcher darf anschließend nur noch als `cfg(test)`-Semantikorakel existieren, nicht als Produktionspfad.

Eine Funktion darf im Produktionsbuild nur noch aus zwei Gründen nicht als Generic Cranelift vorliegen:

1. interner Compilerfehler;
2. fehlende IR-Implementierung, die als harter Testfehler behandelt wird.

„By-reference“, „dynamic call“ oder „static property“ sind keine zulässigen Compile-Rejection-Gründe mehr.

## 2. Resolver und Invocation müssen getrennt werden

Der existierende `JitNativeDispatchTrampoline` ist derzeit Resolver **und** Invoker. Er muss in zwei Ebenen geteilt werden.

### Cold Resolution

Ein Resolver liefert beispielsweise:

```rust
struct ResolvedNativeCall {
    preferred_entry_cell: *const AtomicUsize,
    runtime_view: *const JitNativeRuntimeView,
    binding_plan: u32,
    scope_context: u64,
    generation: u64,
}
```

Er darf:

* Methodenauflösung durchführen,
* Autoload auslösen,
* Cross-Unit-Symbole verlinken,
* einen Binding-Plan auswählen,
* Inline Caches aktualisieren,
* fehlenden Generic-Code kompilieren oder aus dem Cache laden.

Er darf den Callee nicht aufrufen.

### Generated Invocation

Der generierte Caller:

1. bindet die Argumente anhand des gecachten Plans;
2. lädt die Preferred Entry Cell;
3. führt `call_indirect` aus;
4. behandelt `RETURN`, `THROW`, `EXIT` oder `SUSPEND`;
5. setzt den eigenen Cranelift-CFG fort.

Für statisch bekannte Same-Unit-Calls ist gar kein Resolver nötig. Cross-Unit-Calls verwenden die bereits vorhandenen `JitNativeLinkedFunction`-Cells. Methoden und Callables bekommen polymorphe Inline Caches.

Damit verschwinden die heißen Helper-Familien `call_function`, `call_method` und `call_callable` als Ausführungsgrenze.

## 3. By-Reference muss ein Lvalue-Modell sein, kein Optimizer-Verbot

Die vorhandenen Felder `source_slot`, `property_receiver` und `BY_REFERENCE` sind bereits eine brauchbare Grundlage. Sie müssen universell genutzt werden.

Das Modell sollte lauten:

* Ein nicht aliasierter Local darf als SSA-Wert geführt werden.
* Sobald seine Adresse oder Referenzidentität beobachtbar wird, wird nur dieser Local materialisiert.
* Ein By-Reference-Argument übergibt eine stabile Slot-/Reference-ID.
* Nach dem Call werden nur betroffene Alias-Klassen neu geladen.
* By-Reference auf Array-Dim oder Property verwendet einen Lvalue-Resolver, der eine stabile Reference-ID liefert.
* Typed Properties, Magic Properties und Visibility-Prüfungen dürfen dabei einen lokalen Runtime-Leaf verwenden.
* Der anschließend aufgerufene PHP-Callee bleibt trotzdem Cranelift-Code.
* Return-by-Reference liefert eine Reference-ID mit entsprechendem Status zurück.

Damit wird aus:

```text
ein By-Ref-Call
    → ganze Funktion nicht optimierbar
```

folgendes:

```text
ein By-Ref-Call
    → betroffene SSA-Werte materialisieren
    → Reference-ID binden
    → generierten Callee aufrufen
    → betroffene Werte neu laden
```

Das ist die Voraussetzung dafür, die 14.761 beobachteten Rejections tatsächlich zu eliminieren.

## 4. Include, Autoload und Dynamic Code dürfen nur noch laden und verlinken

Der aktuelle Include-Pfad kompiliert und registriert ein Unit, ruft dessen Entry anschließend aber aus Rust über `invoke_native_function` auf.

Das muss geteilt werden:

```text
resolve/include:
    path auflösen
    source/artifact laden
    Unit registrieren
    Symbol- und Runtime-Views veröffentlichen
    Include-Locals vorbereiten
    Entry Cell zurückgeben

generated caller:
    Entry Cell laden
    Unit Entry nativ aufrufen
    Include-Locals zurückschreiben
    im ursprünglichen Cranelift-CFG fortsetzen
```

Dasselbe gilt für:

* Autoload,
* Closures aus dynamischen Units,
* `eval`,
* konditionale Deklarationen,
* Cross-Unit-Methoden.

Zusätzlich braucht Phrust ein **Deployment Native Image**:

* Maschinen-Code-Key nur aus Source-/IR-Hash, Engine-ABI, Target-ISA und Tier;
* keine externen Signaturen oder aktuellen Receiver-Layouts im Generic-Artefakt;
* Verlinkung ausschließlich über Entry-Cells und versionierte Tabellen;
* alle Dateien können ohne Ausführung der Anwendung im Build- oder Startup-Schritt vorkompiliert werden;
* Deklarationen werden trotzdem erst bei tatsächlichem Include semantisch veröffentlicht;
* `eval` verwendet denselben Pfad über einen Source-Hash-Cache.

Das ist kein Warm Runner. Es wird keine PHP-Anwendung ausgeführt und keine WordPress-spezifische Annahme verwendet.

## 5. Builtins brauchen eine direkte Native-Builtin-ABI

Ein PHP-Builtin darf in Rust implementiert sein. Auch php-src ruft C-Implementierungen auf. Problematisch ist nicht Rust, sondern der aktuelle vollständige Baseline-Control- und Dispatch-Pfad.

Jedes Builtin braucht einen stabilen direkten Entry:

```rust
extern "C" fn(
    runtime: *mut NativeRequestFastState,
    args: *const BoundNativeArg,
    argc: u32,
) -> JitNativeControlResult
```

Dabei gilt:

* kein Name-Lookup pro Call;
* kein `PreparedNativeBuiltin::for_dense_id` im heißen Pfad;
* kein Wechsel in `with_baseline_native_context_for`;
* kein Aufbau eines vollständigen generischen Callframes;
* kein Rust-`Value`, solange die direkte Repräsentation ausreicht;
* Arity, Defaults und By-Ref-Maske liegen im Callsite- oder Binding-Plan;
* Status und Wert werden registerbasiert zurückgegeben.

Nur kleine, sehr häufige Operationen sollten zusätzlich inline werden, etwa bekannte `is_*`-Checks, `strlen` auf direktem String oder `count` auf einer nachgewiesenen direkten Array-Form. Das Ziel ist nicht, hunderte Builtins in riesigen Caller-Code zu kopieren.

Die aktuellen Exact-Builtin-Handler können dabei weiterverwendet werden. Sie dürfen nur nicht mehr von einem „total resource publication plan“ abhängig sein.

## 6. Lowering und Codeform müssen fundamental kompakter werden

Die 40 Fragmente und 11.899 CLIF-Blöcke sind nicht durch größere Grenzwerte zu lösen. Notwendig sind:

* ein gemeinsamer Cold-Block pro Funktion für Runtime Error;
* ein gemeinsamer Throw-/Unwind-Block;
* ein gemeinsamer Cleanup-Epilog;
* ausgelagerte Slow Paths statt kopierter Branch-Bäume;
* lokale Capacity-Checks statt vorab bewiesener Gesamtallokationen;
* Liveness- und Deopt-Metadaten statt permanent generierter Snapshot-Logik;
* ein kompakter Ownership-Bitmap oder Cleanup-Stack statt individueller Guard-/Release-Bäume für jeden Local;
* ein vollständiger Deopt-State nur dann, wenn tatsächlich deoptimiert oder suspendiert wird;
* keine vollständige Transition-Struktur an jedem gewöhnlichen Call.

Der derzeitige `JitDeoptState` reserviert feste Bereiche für bis zu 256 Locals und 64 Register und enthält zusätzlich eine komplette Runtime-View-Repräsentation. Diese Struktur darf nicht als normaler Call- oder Fragmentzustand behandelt werden.

Produktionsziel muss wieder sein:

* normalerweise eine Cranelift-Funktion pro PHP-Funktion;
* wenige zusätzliche Resume-Entries für Generatoren/Fibers;
* Fragmentierung nur an echten strukturellen Grenzen;
* nie wieder eine künstliche Grenze nach 16 Instruktionen.

---

# Der neue Optimizer

Erst auf dieser vollständigen Generic-Cranelift-Basis ist ein Optimizer sinnvoll.

## Kein Admission Gate mehr

`reject_unpublished_optimizer_boundaries()` wird vollständig entfernt.

An seine Stelle tritt eine Klassifikation pro Instruktion:

```rust
enum LoweringMode {
    Inline,
    GuardedInline,
    DirectRuntimeLeaf,
    ResolvedNativeCall,
}
```

Es gibt keinen Modus `RejectFunction`.

`NativeOptimizingAdmission` wird auf echte optionale Annahmen reduziert:

* beobachtete Value-Klasse,
* Array-/Object-Layout,
* Callsite-Ziel und Generation,
* Alias-/Escape-Information,
* Range-Information,
* Ownership.

Nicht bewiesene Informationen führen zu einem Guard oder einem generischen Native-Leaf, nicht zur Ablehnung.

## Deopt geht in Generic Cranelift, nicht in Rust

Ein Spekulationsfehler darf:

* lokal den Generic-Leaf aufrufen und anschließend im optimierten Code fortfahren;
* oder in einen Resume-Entry des Generic-Cranelift-Bodys wechseln.

Er darf nicht:

* einen PHP-Call-Dispatcher aufrufen;
* die gesamte Funktion in der Baseline erneut ausführen;
* einen Rust-Executor als semantischen Fallback verwenden.

## SSA und References

Die Optimierung arbeitet mit drei Local-Zuständen:

```text
SSA-only
Materialized native slot
Aliased reference slot
```

By-Reference materialisiert nur die betroffene Alias-Klasse. Danach kann der Optimizer andere Locals weiterhin in Registern halten. Das jetzige Ergebnis von null SSA-promoteten Locals und Registern muss dadurch verschwinden.

## Optimierungs-Publication

Jede Funktion hat mindestens:

```text
generic_entry_cell
preferred_entry_cell
```

Beide verwenden dieselbe ABI. Der Optimizer veröffentlicht atomar in `preferred_entry_cell`. Laufende Calls bleiben auf ihrer Version, spätere Calls sehen die neue Version.

Ein heißer Funktionsbody darf nicht wegen einer dynamischen Callsite dauerhaft ohne optimierte Version bleiben.

---

# Richtige Reihenfolge der Implementierung

## Cut 0: Semantik und Messbarkeit

Parallel zur Architekturarbeit müssen sofort behoben werden:

* `count(uninitialized)`,
* der Stack Overflow,
* ein vollständiger HTTP-200-WordPress-Request mit identischem Body-Hash.

Aber diese Fehler dürfen nicht erneut als Begründung dienen, zuerst Monate an Baseline-Helpern zu optimieren.

Zusätzlich werden eindeutige Counter eingeführt:

```text
rust_userland_invocations
rust_call_dispatch_invocations
rust_dynamic_unit_invocations
generated_to_generated_calls
generic_cranelift_body_entries
optimized_cranelift_body_entries
resolver_misses
binding_slow_paths
deoptimizations
```

Ein bisheriger „native entry“-Counter reicht nicht, weil auch der dispatcher-dominierte Pfad native Entry-Stubs verwendet.

## Cut 1: Native Call Spine

Als Erstes müssen funktionieren:

* Same-Unit-Function-Calls,
* Cross-Unit-Function-Calls,
* typed parameters,
* Defaults,
* Variadics,
* Named Arguments,
* By-Reference-Locals,
* Return-by-Reference.

Alle gehen nach Auflösung von generiertem Caller zu generiertem Callee.

Erst dieser Cut bricht den dominanten 29,33-Sekunden-`call_function`-Block.

## Cut 2: PHP Application Graph

Danach:

* Methoden und statische Methoden mit PIC,
* Closures und Callables,
* Includes und Autoload,
* Dynamic Units,
* Constructors,
* Magic Calls,
* Exceptions über native Statuswerte.

Dieser Cut entfernt `call_method` und `dynamic_code` als heiße Rust-Ausführungsgrenzen.

## Cut 3: Builtin ABI und kompakte Codeform

Danach:

* direkte Builtin-Entries;
* gemeinsame Cold Stubs;
* lazy Deopt-State;
* Entfernung der 16-Instruktionen-Splits;
* Unit- oder SCC-weises Kompilieren mehrerer Funktionen in einem Cranelift-Modul;
* stabiler Source-basierter Artifact-Key.

## Cut 4: Optimizing Working Set

Erst jetzt:

* SSA-Promotion,
* Array-/String-Fastpaths,
* Method-/Callable-Spezialisierung,
* Hot-Call-Inlining,
* Loop-Spezialisierung,
* Range- und Type-Guards,
* lokale Deoptimierung zum Generic-Cranelift-Tier.

Tiering wird unabhängig vom Request-Ergebnis angestoßen. Die Ein-Job-Queue wird durch eine deduplizierende Work-Queue ersetzt, die Funktionen oder Units bündelt. Ein fehlgeschlagener HTTP-Request darf die Publication nicht blockieren.

---

# Nicht verhandelbare Gates

## Semantik

Für den bestehenden WordPress-Vergleich:

* HTTP 200;
* 70.949 Bytes;
* SHA-256 exakt `7a34e150c5304aea4744a0e4f3b4fd70c4309cca411902513478a9ba7a196072`;
* kein Stack Overflow;
* keine uninitialisierte `count()`-Semantik;
* keine WordPress-Modifikation.

## Architektur

Bei warmem Native-Cache:

* `rust_userland_invocations = 0`;
* `rust_call_dispatch_invocations = 0` für bereits aufgelöste Calls;
* `rust_dynamic_unit_invocations = 0`;
* Same-Unit-Calls verlassen generierten Code nicht;
* Cross-Unit-Resolver nur beim ersten Link-Miss;
* 100 % der PHP-Funktionsbody-Entries sind Generic oder Optimized Cranelift;
* jede heiße Funktion oberhalb des Tiering-Schwellwerts besitzt einen Optimizer-Body;
* By-Reference ist kein Optimizer-Rejection-Grund mehr.

## Kompilierung

Beim warmen WordPress-Request:

* null Kompilierungen;
* null Replan-Runden;
* kein externer Signaturzustand im Generic-Code-Cache-Key;
* eine eindeutige Generic-Code-Version pro Source-Funktion und Target-ABI.

Für `wp_should_add_elements_class_name` muss als erstes strukturelles Gate gelten:

* höchstens ein bis drei natürliche native Funktionen statt 40 Fragmente;
* höchstens etwa 2.000 CLIF-Blöcke;
* höchstens etwa 128 KiB Code;
* keine Pre-Regalloc-Replan-Runde;
* Compile-Zeit auf demselben Host unter 100 ms.

Diese Werte sind noch kein finales Optimum. Sie verhindern lediglich, dass die gegenwärtige Codeexplosion erneut als „erfolgreiche Kompilierung“ akzeptiert wird.

## Performance

Auf exakt deinem vorhandenen Vergleich:

* p50 höchstens **63,069 ms**;
* p95 höchstens **97,587 ms**;
* mindestens **14,868 Requests/s** bei Concurrency 1;
* HTTP-Body identisch;
* Instrumentierung aus.

Zusätzlich:

* `call_function`, `call_method`, `dynamic_code` und `reference_bind` zusammen unter 1 % der Laufzeit;
* keine generische `call_builtin_direct`-Dispatcherzeit;
* Optimizer- und Generic-Cranelift-Zeit separat ausgewiesen;
* keine Kategorie „other native mechanisms“ ohne weitere Attribution.

---

# Was ab jetzt ausdrücklich nicht mehr passieren darf

* Kein neues `JIT_CRANELIFT_REJECT_*` für gewöhnliche PHP-Semantik.
* Keine zusätzlichen Publication-Proofs als Voraussetzung für Funktionskompilierung.
* Keine weiteren Fragmentgrenzen zur Beherrschung aufgeblähter Lowerings.
* Kein Wrapper um `jit_baseline_native_call_dispatch_impl`.
* Kein „schnellerer“ Rust-Call-Dispatcher als Endlösung.
* Kein erneutes synchrones Kompilieren aller Funktionen während eines Requests.
* Kein Maschinen-Code-Key, der aktuelle externe Linkage oder Receiver-Layouts enthält.
* Kein Hintergrundtiering erst nach erfolgreichem Request.
* Kein Warm Runner und keine WordPress-spezifische Sonderbehandlung.
* Kein Rückzug auf den früheren Faktor-15-Zustand.

Diese Regeln sollten statisch in den vorhandenen Source-Integrity- und Product-Surface-Checks abgesichert werden. Der alte Dispatcher darf im Produktionsartefakt nicht mehr importiert werden.

---

# Warum diese Strategie substanziell anders ist

Die fünf dominanten Helper-Familien aus deinem Profil summieren sich auf **96,141 Sekunden**, also **93,8 %** der gemessenen Helper-Zeit. Die vorgeschlagene Architektur versucht nicht, diese fünf Pfade jeweils um 10 oder 20 % zu beschleunigen. Sie entfernt vier davon vollständig als Ausführungsgrenze und ersetzt den fünften durch direkte Builtin-Entries.

Gleichzeitig beseitigt sie:

* die 3.997 request-synchronen Compiles,
* die externe Signatur als Codevariante,
* die 39.968 Cranelift-zu-Rust-/Fragment-Übergänge,
* die all-or-nothing Optimizer-Rejections,
* die 16-Instruktionen-Fragmentierung,
* die Abhängigkeit der Optimizer-Publication von einem erfolgreichen Request.

Ob der anschließend sichtbare Rest unmittelbar für Faktor 2 reicht, lässt sich vor dem neuen Residualprofil nicht seriös garantieren. Diese Architektur ist aber der erste Ansatz, der den tatsächlich gemessenen Verlust von über 93 % direkt beseitigt und zugleich sicherstellt, dass WordPress nicht erneut dauerhaft im Baseline-Dispatcher hängen bleibt.

Der erste Implementierungs-PR muss deshalb der **Native Call Spine** sein: Resolver und Invocation trennen, das bestehende Call-/Lvalue-ABI universell machen und Same-Unit-, Cross-Unit- sowie By-Reference-Calls von generiertem Code direkt in generierten Code führen. Bis dieser Cut nachweislich funktioniert, sollte kein weiterer Helper- oder Fragment-Optimierungs-Patch angenommen werden.

