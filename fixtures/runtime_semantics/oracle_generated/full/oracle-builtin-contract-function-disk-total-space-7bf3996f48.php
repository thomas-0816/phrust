<?php
// oracle-probe: id=oracle-builtin-contract-function-disk-total-space-7bf3996f48 area=builtin_contract kind=function symbol=disk_total_space source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-disk-total-space-7bf3996f48 failure_category=builtin_contract
$name = "disk_total_space";
echo function_exists($name) ? "available\n" : "missing\n";
