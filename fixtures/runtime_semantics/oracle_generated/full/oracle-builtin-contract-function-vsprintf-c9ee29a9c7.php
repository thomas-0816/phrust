<?php
// oracle-probe: id=oracle-builtin-contract-function-vsprintf-c9ee29a9c7 area=builtin_contract kind=function symbol=vsprintf source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-vsprintf-c9ee29a9c7 failure_category=builtin_contract
$name = "vsprintf";
echo function_exists($name) ? "available\n" : "missing\n";
