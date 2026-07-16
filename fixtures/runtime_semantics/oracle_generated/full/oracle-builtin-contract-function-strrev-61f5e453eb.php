<?php
// oracle-probe: id=oracle-builtin-contract-function-strrev-61f5e453eb area=builtin_contract kind=function symbol=strrev source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strrev-61f5e453eb failure_category=builtin_contract
$name = "strrev";
echo function_exists($name) ? "available\n" : "missing\n";
