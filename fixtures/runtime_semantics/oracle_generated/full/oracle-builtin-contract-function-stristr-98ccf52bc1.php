<?php
// oracle-probe: id=oracle-builtin-contract-function-stristr-98ccf52bc1 area=builtin_contract kind=function symbol=stristr source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stristr-98ccf52bc1 failure_category=builtin_contract
$name = "stristr";
echo function_exists($name) ? "available\n" : "missing\n";
