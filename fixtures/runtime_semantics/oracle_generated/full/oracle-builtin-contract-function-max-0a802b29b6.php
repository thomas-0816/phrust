<?php
// oracle-probe: id=oracle-builtin-contract-function-max-0a802b29b6 area=builtin_contract kind=function symbol=max source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-max-0a802b29b6 failure_category=builtin_contract
$name = "max";
echo function_exists($name) ? "available\n" : "missing\n";
