<?php
// oracle-probe: id=oracle-builtin-contract-function-gc-status-cdbe3e519b area=builtin_contract kind=function symbol=gc_status source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-gc-status-cdbe3e519b failure_category=builtin_contract
$name = "gc_status";
echo function_exists($name) ? "available\n" : "missing\n";
