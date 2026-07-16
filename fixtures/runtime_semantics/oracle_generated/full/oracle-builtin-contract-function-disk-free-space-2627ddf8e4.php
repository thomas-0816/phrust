<?php
// oracle-probe: id=oracle-builtin-contract-function-disk-free-space-2627ddf8e4 area=builtin_contract kind=function symbol=disk_free_space source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-disk-free-space-2627ddf8e4 failure_category=builtin_contract
$name = "disk_free_space";
echo function_exists($name) ? "available\n" : "missing\n";
