<?php
// oracle-probe: id=oracle-builtin-contract-function-strnatcmp-754e938578 area=builtin_contract kind=function symbol=strnatcmp source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strnatcmp-754e938578 failure_category=builtin_contract
$name = "strnatcmp";
echo function_exists($name) ? "available\n" : "missing\n";
