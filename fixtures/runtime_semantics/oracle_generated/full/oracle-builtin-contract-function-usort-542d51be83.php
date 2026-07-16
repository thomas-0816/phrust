<?php
// oracle-probe: id=oracle-builtin-contract-function-usort-542d51be83 area=builtin_contract kind=function symbol=usort source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-usort-542d51be83 failure_category=builtin_contract
$name = "usort";
echo function_exists($name) ? "available\n" : "missing\n";
