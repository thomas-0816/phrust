<?php
// oracle-probe: id=oracle-builtin-contract-function-acos-814900124e area=builtin_contract kind=function symbol=acos source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-acos-814900124e failure_category=builtin_contract
$name = "acos";
echo function_exists($name) ? "available\n" : "missing\n";
