<?php
// oracle-probe: id=oracle-builtin-contract-function-file-7e094fad15 area=builtin_contract kind=function symbol=file source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-file-7e094fad15 failure_category=builtin_contract
$name = "file";
echo function_exists($name) ? "available\n" : "missing\n";
