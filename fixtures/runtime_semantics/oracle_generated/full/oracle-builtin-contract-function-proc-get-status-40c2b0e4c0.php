<?php
// oracle-probe: id=oracle-builtin-contract-function-proc-get-status-40c2b0e4c0 area=builtin_contract kind=function symbol=proc_get_status source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-proc-get-status-40c2b0e4c0 failure_category=builtin_contract
$name = "proc_get_status";
echo function_exists($name) ? "available\n" : "missing\n";
