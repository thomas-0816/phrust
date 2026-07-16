<?php
// oracle-probe: id=oracle-builtin-contract-function-file-exists-e2f847baec area=builtin_contract kind=function symbol=file_exists source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-file-exists-e2f847baec failure_category=builtin_contract
$name = "file_exists";
echo function_exists($name) ? "available\n" : "missing\n";
