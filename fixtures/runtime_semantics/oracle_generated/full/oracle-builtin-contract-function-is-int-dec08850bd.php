<?php
// oracle-probe: id=oracle-builtin-contract-function-is-int-dec08850bd area=builtin_contract kind=function symbol=is_int source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-int-dec08850bd failure_category=builtin_contract
$name = "is_int";
echo function_exists($name) ? "available\n" : "missing\n";
