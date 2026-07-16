<?php
// oracle-probe: id=oracle-builtin-contract-function-gc-enable-4ea011191b area=builtin_contract kind=function symbol=gc_enable source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-gc-enable-4ea011191b failure_category=builtin_contract
$name = "gc_enable";
echo function_exists($name) ? "available\n" : "missing\n";
