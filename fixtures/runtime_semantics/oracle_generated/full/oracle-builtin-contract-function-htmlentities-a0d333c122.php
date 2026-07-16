<?php
// oracle-probe: id=oracle-builtin-contract-function-htmlentities-a0d333c122 area=builtin_contract kind=function symbol=htmlentities source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-htmlentities-a0d333c122 failure_category=builtin_contract
$name = "htmlentities";
echo function_exists($name) ? "available\n" : "missing\n";
