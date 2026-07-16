<?php
// oracle-probe: id=oracle-builtin-contract-function-is-bool-9ebb374ab8 area=builtin_contract kind=function symbol=is_bool source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-bool-9ebb374ab8 failure_category=builtin_contract
$name = "is_bool";
echo function_exists($name) ? "available\n" : "missing\n";
