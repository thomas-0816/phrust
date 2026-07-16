<?php
// oracle-probe: id=oracle-builtin-contract-function-debug-print-backtrace-2805e4084c area=builtin_contract kind=function symbol=debug_print_backtrace source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-debug-print-backtrace-2805e4084c failure_category=builtin_contract
$name = "debug_print_backtrace";
echo function_exists($name) ? "available\n" : "missing\n";
