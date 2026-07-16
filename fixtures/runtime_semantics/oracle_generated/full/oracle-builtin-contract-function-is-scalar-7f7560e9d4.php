<?php
// oracle-probe: id=oracle-builtin-contract-function-is-scalar-7f7560e9d4 area=builtin_contract kind=function symbol=is_scalar source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-scalar-7f7560e9d4 failure_category=builtin_contract
$name = "is_scalar";
echo function_exists($name) ? "available\n" : "missing\n";
