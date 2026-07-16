<?php
// oracle-probe: id=oracle-builtin-contract-function-uksort-8a438d9243 area=builtin_contract kind=function symbol=uksort source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-uksort-8a438d9243 failure_category=builtin_contract
$name = "uksort";
echo function_exists($name) ? "available\n" : "missing\n";
