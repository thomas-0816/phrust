<?php
// oracle-probe: id=oracle-builtin-contract-function-gc-mem-caches-d977b05070 area=builtin_contract kind=function symbol=gc_mem_caches source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-gc-mem-caches-d977b05070 failure_category=builtin_contract
$name = "gc_mem_caches";
echo function_exists($name) ? "available\n" : "missing\n";
