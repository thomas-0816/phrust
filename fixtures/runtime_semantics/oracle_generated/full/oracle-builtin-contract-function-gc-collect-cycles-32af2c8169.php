<?php
// oracle-probe: id=oracle-builtin-contract-function-gc-collect-cycles-32af2c8169 area=builtin_contract kind=function symbol=gc_collect_cycles source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-gc-collect-cycles-32af2c8169 failure_category=builtin_contract
$name = "gc_collect_cycles";
echo function_exists($name) ? "available\n" : "missing\n";
