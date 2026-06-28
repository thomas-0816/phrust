# PHPT Module Plan

This directory contains the functional module plan for PHPT-driven runtime completion. The order is based on core language dependencies, failure volume, and expected leverage across later modules.

| Priority | Module | Corpus | PASS | SKIP | FAIL | BORK | Next step |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 1 | [phpt.foundation](phpt.foundation.md) | 0 | 0 | 0 | 0 | 0 | Keep committed baseline, corpus, and source-integrity manifests consistent. |
| 2 | [phpt.runner](phpt.runner.md) | 0 | 0 | 0 | 0 | 437 | Reduce runner-owned BORKs before attributing failures to the engine. |
| 3 | [phpt.cli](phpt.cli.md) | 350 | 3 | 17 | 256 | 0 | Keep target invocation deterministic for upstream PHPT execution. |
| 4 | [zend.basic](zend.basic.md) | 3509 | 434 | 40 | 3027 | 0 | Keep the selected zend.basic gate green while later modules expand runtime semantics. |
| 5 | [operators.conversions](operators.conversions.md) | 129 | 16 | 6 | 107 | 0 | Keep the selected scalar conversion gate green while later modules expand arrays, objects, and diagnostics. |
| 6 | [diagnostics.output](diagnostics.output.md) | 0 | 0 | 0 | 0 | 0 | Centralize runtime diagnostic rendering and continuation semantics. |
| 7 | [strings.literals](strings.literals.md) | 9 | 0 | 0 | 9 | 0 | Separate frontend literal gaps from runtime string builtin gaps. |
| 8 | [arrays.references](arrays.references.md) | 273 | 26 | 1 | 246 | 0 | Close array data-model and reference/COW gaps before array builtins. |
| 9 | [zend.functions](zend.functions.md) | 887 | 85 | 53 | 727 | 0 | Prompt 13 selected generated function/callable contracts are closed and green. |
| 10 | [objects.classes](objects.classes.md) | 2136 | 178 | 33 | 1924 | 0 | Stabilize constructor/property/method basics before magic behavior. |
| 11 | [filesystem.streams](filesystem.streams.md) | 1194 | 66 | 217 | 849 | 0 | Keep filesystem policy root-constrained and deterministic. |
| 12 | [standard.arrays](standard.arrays.md) | 821 | 218 | 7 | 595 | 0 | Implement array builtins after array data model gaps are closed. |
| 13 | [standard.strings](standard.strings.md) | 727 | 352 | 42 | 308 | 0 | Close common binary-safe string functions against Reference PHP. |
| 14 | [standard.math](standard.math.md) | 171 | 14 | 11 | 146 | 0 | Use php-src arginfo and Reference PHP for edge-case numeric behavior. |
| 15 | [standard.variables](standard.variables.md) | 446 | 23 | 74 | 348 | 0 | Stabilize var_dump/print_r/serialization-adjacent value rendering. |
| 16 | [standard.serialization](standard.serialization.md) | 126 | 16 | 2 | 107 | 0 | Implement serialization after arrays/objects are stable. |
| 17 | [json](json.md) | 88 | 10 | 1 | 77 | 0 | Prompt 17 selected JSON contracts are closed and green. |
| 18 | [pcre](pcre.md) | 165 | 41 | 5 | 117 | 0 | Use PCRE2 while documenting unsupported modifier/callout gaps. |
| 19 | [date](date.md) | 687 | 14 | 12 | 661 | 0 | Stabilize timezone persistence and common formatting/parsing. |
| 20 | [spl](spl.md) | 520 | 39 | 3 | 478 | 0 | Build on stable object, array, iterator, and filesystem layers. |
| 21 | [reflection](reflection.md) | 304 | 11 | 1 | 292 | 0 | Expose generated arginfo and semantic metadata through Reflection APIs. |
| 22 | [extension.policy](extension.policy.md) | 9006 | 468 | 525 | 7757 | 0 | Classify extension failures without hiding them from full regression. |
