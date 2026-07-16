<?php
// oracle-probe: id=oracle-builtin-contract-function-headers-sent-8b3d604d86 area=builtin_contract kind=function symbol=headers_sent source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-headers-sent-8b3d604d86 failure_category=builtin_contract
$name = "headers_sent";
echo function_exists($name) ? "available\n" : "missing\n";
