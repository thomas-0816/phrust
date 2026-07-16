<?php
// oracle-probe: id=oracle-builtin-contract-function-clearstatcache-c7fd145571 area=builtin_contract kind=function symbol=clearstatcache source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-clearstatcache-c7fd145571 failure_category=builtin_contract
$name = "clearstatcache";
echo function_exists($name) ? "available\n" : "missing\n";
