# PHPT Triage

Baseline `20260624T210848Z` covers 21548 PHPTs: 1056 PASS, 64 SKIP, 19973 FAIL, 455 BORK.

Per-module PASS/SKIP counts are based on the latest available full-run results.

## Top Failing Modules

| Module | Priority | Corpus | PASS | SKIP | FAIL | BORK | Known non-green |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| phpt.foundation | 1 | 0 | 0 | 0 | 0 | 0 | 0 |
| phpt.runner | 2 | 0 | 0 | 0 | 0 | 437 | 437 |
| phpt.cli | 3 | 350 | 3 | 17 | 256 | 0 | 275 |
| zend.basic | 4 | 3509 | 434 | 40 | 3027 | 0 | 3227 |
| operators.conversions | 5 | 129 | 16 | 6 | 107 | 0 | 118 |
| diagnostics.output | 6 | 0 | 0 | 0 | 0 | 0 | 0 |
| strings.literals | 7 | 9 | 0 | 0 | 9 | 0 | 9 |
| arrays.references | 8 | 273 | 26 | 1 | 246 | 0 | 260 |
| zend.functions | 9 | 887 | 85 | 53 | 727 | 0 | 818 |
| objects.classes | 10 | 2136 | 178 | 33 | 1924 | 0 | 2000 |
| filesystem.streams | 11 | 1194 | 66 | 217 | 849 | 0 | 1100 |
| standard.arrays | 12 | 821 | 218 | 7 | 595 | 0 | 735 |
| standard.strings | 13 | 727 | 352 | 42 | 308 | 0 | 621 |
| standard.math | 14 | 171 | 14 | 11 | 146 | 0 | 163 |
| standard.variables | 15 | 446 | 23 | 74 | 348 | 0 | 435 |
| standard.serialization | 16 | 126 | 16 | 2 | 107 | 0 | 115 |
| json | 17 | 88 | 10 | 1 | 77 | 0 | 79 |
| pcre | 18 | 165 | 41 | 5 | 117 | 0 | 126 |
| date | 19 | 687 | 14 | 12 | 661 | 0 | 675 |
| spl | 20 | 520 | 39 | 3 | 478 | 0 | 493 |

## Top Failure Clusters

| Cluster | Count |
| --- | ---: |
| runtime-error-or-diagnostic | 11402 |
| runtime-unsupported-feature | 6185 |
| runtime-output-mismatch | 2315 |
| needs-triage | 320 |
| frontend-parse-or-compile | 187 |
| runtime-timeout | 19 |

## Top Unsupported Feature Guesses

| Guess | Count |
| --- | ---: |
| runtime-unsupported-feature | 6185 |

## BORK Subclasses

| Subclass | Count |
| --- | ---: |
| malformed-or-non-utf8-phpt | 313 |
| missing-target-cli-capability | 96 |
| unsupported-section | 21 |
| unsupported-expectation | 10 |
| other-bork | 8 |
| unsupported-file-external | 6 |
| unsupported-runner-io | 1 |

## Next Module Candidates

| Rank | Module | Reason |
| ---: | --- | --- |
| 1 | phpt.runner | 437 non-green, leverage 98 |
| 2 | phpt.cli | 256 non-green, leverage 96 |
| 3 | zend.basic | 3027 non-green, leverage 94 |
| 4 | operators.conversions | 107 non-green, leverage 92 |
| 5 | strings.literals | 9 non-green, leverage 88 |
| 6 | arrays.references | 246 non-green, leverage 86 |
| 7 | zend.functions | 727 non-green, leverage 84 |
| 8 | objects.classes | 1924 non-green, leverage 82 |
| 9 | filesystem.streams | 849 non-green, leverage 80 |
| 10 | standard.arrays | 595 non-green, leverage 78 |

## Raw Corpus Module Counts

| Module | Corpus | PASS | SKIP | FAIL | BORK | Known non-green |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| zend | 5305 | 598 | 47 | 4652 | 7 | 4916 |
| unknown | 1419 | 245 | 36 | 1120 | 17 | 1286 |
| standard | 1140 | 99 | 124 | 894 | 23 | 1085 |
| filesystem | 947 | 59 | 194 | 638 | 56 | 923 |
| dom | 879 | 7 | 14 | 851 | 7 | 879 |
| standard.arrays | 871 | 227 | 7 | 636 | 1 | 786 |
| spl | 784 | 48 | 3 | 732 | 0 | 751 |
| date | 689 | 14 | 12 | 663 | 0 | 677 |
| standard.strings | 741 | 357 | 42 | 317 | 25 | 659 |
| soap | 589 | 0 | 16 | 567 | 6 | 577 |
| phar | 553 | 3 | 6 | 403 | 141 | 552 |
| reflection | 494 | 23 | 1 | 470 | 0 | 473 |
| intl | 477 | 0 | 18 | 458 | 0 | 467 |
| opcache | 593 | 220 | 8 | 364 | 0 | 449 |
| mysqli | 442 | 2 | 4 | 429 | 4 | 442 |
| mbstring | 420 | 3 | 36 | 360 | 21 | 414 |
| sapi | 347 | 2 | 17 | 254 | 73 | 346 |
| gd | 312 | 1 | 55 | 255 | 0 | 310 |
| session | 260 | 3 | 0 | 254 | 2 | 260 |
| streams | 252 | 7 | 23 | 216 | 6 | 244 |
| openssl | 208 | 1 | 26 | 181 | 0 | 203 |
| uri | 191 | 0 | 0 | 191 | 0 | 191 |
| curl | 170 | 0 | 4 | 164 | 0 | 170 |
| bcmath | 166 | 0 | 1 | 165 | 0 | 166 |
| pdo_mysql | 159 | 0 | 1 | 158 | 0 | 159 |
| simplexml | 157 | 0 | 2 | 155 | 0 | 157 |
| zend_test | 148 | 1 | 4 | 143 | 0 | 147 |
| ldap | 140 | 0 | 9 | 130 | 1 | 140 |
| zlib | 143 | 3 | 5 | 123 | 12 | 140 |
| pdo | 137 | 0 | 117 | 18 | 2 | 137 |
| pcre | 165 | 41 | 5 | 117 | 2 | 128 |
| filter | 120 | 0 | 0 | 117 | 0 | 120 |
| sockets | 106 | 0 | 37 | 69 | 0 | 106 |
| ffi | 106 | 2 | 2 | 102 | 0 | 105 |
| zip | 103 | 3 | 14 | 85 | 1 | 102 |
| pgsql | 100 | 0 | 4 | 96 | 0 | 100 |
| gmp | 99 | 0 | 2 | 97 | 0 | 99 |
| sqlite3 | 96 | 0 | 7 | 89 | 0 | 96 |
| exif | 93 | 0 | 0 | 92 | 1 | 93 |
| pdo_sqlite | 80 | 0 | 6 | 73 | 1 | 80 |
