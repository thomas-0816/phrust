<?php
// oracle-probe: id=oracle-builtin-contract-function-is-resource-b976235de5 area=builtin_contract kind=function symbol=is_resource source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-resource-b976235de5 failure_category=builtin_contract
$name = "is_resource";
echo function_exists($name) ? "available\n" : "missing\n";
