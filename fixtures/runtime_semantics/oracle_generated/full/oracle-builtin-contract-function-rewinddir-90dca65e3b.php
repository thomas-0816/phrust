<?php
// oracle-probe: id=oracle-builtin-contract-function-rewinddir-90dca65e3b area=builtin_contract kind=function symbol=rewinddir source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-rewinddir-90dca65e3b failure_category=builtin_contract
$name = "rewinddir";
echo function_exists($name) ? "available\n" : "missing\n";
