<?php
// oracle-probe: id=oracle-builtin-contract-function-asort-d501507ad0 area=builtin_contract kind=function symbol=asort source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-asort-d501507ad0 failure_category=builtin_contract
$name = "asort";
echo function_exists($name) ? "available\n" : "missing\n";
