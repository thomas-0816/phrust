<?php
// oracle-probe: id=oracle-builtin-contract-function-gc-enabled-0709391db8 area=builtin_contract kind=function symbol=gc_enabled source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-gc-enabled-0709391db8 failure_category=builtin_contract
$name = "gc_enabled";
echo function_exists($name) ? "available\n" : "missing\n";
