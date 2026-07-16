<?php
// oracle-probe: id=oracle-builtin-contract-function-is-iterable-44fef8c4c1 area=builtin_contract kind=function symbol=is_iterable source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-iterable-44fef8c4c1 failure_category=builtin_contract
$name = "is_iterable";
echo function_exists($name) ? "available\n" : "missing\n";
