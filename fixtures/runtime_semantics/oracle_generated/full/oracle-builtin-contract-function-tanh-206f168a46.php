<?php
// oracle-probe: id=oracle-builtin-contract-function-tanh-206f168a46 area=builtin_contract kind=function symbol=tanh source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-tanh-206f168a46 failure_category=builtin_contract
$name = "tanh";
echo function_exists($name) ? "available\n" : "missing\n";
