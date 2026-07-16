<?php
// oracle-probe: id=oracle-builtin-contract-function-ob-start-6f68a2f3f8 area=builtin_contract kind=function symbol=ob_start source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ob-start-6f68a2f3f8 failure_category=builtin_contract
$name = "ob_start";
echo function_exists($name) ? "available\n" : "missing\n";
