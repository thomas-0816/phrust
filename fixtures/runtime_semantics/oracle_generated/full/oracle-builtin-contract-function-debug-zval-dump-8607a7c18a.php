<?php
// oracle-probe: id=oracle-builtin-contract-function-debug-zval-dump-8607a7c18a area=builtin_contract kind=function symbol=debug_zval_dump source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-debug-zval-dump-8607a7c18a failure_category=builtin_contract
$name = "debug_zval_dump";
echo function_exists($name) ? "available\n" : "missing\n";
