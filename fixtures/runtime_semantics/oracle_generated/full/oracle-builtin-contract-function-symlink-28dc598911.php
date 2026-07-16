<?php
// oracle-probe: id=oracle-builtin-contract-function-symlink-28dc598911 area=builtin_contract kind=function symbol=symlink source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-symlink-28dc598911 failure_category=builtin_contract
$name = "symlink";
echo function_exists($name) ? "available\n" : "missing\n";
