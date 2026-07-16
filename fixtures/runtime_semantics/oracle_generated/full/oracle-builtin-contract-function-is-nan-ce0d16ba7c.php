<?php
// oracle-probe: id=oracle-builtin-contract-function-is-nan-ce0d16ba7c area=builtin_contract kind=function symbol=is_nan source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-nan-ce0d16ba7c failure_category=builtin_contract
$name = "is_nan";
echo function_exists($name) ? "available\n" : "missing\n";
