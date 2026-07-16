<?php
// oracle-probe: id=oracle-builtin-contract-function-print-r-bb2e737b7e area=builtin_contract kind=function symbol=print_r source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-print-r-bb2e737b7e failure_category=builtin_contract
$name = "print_r";
echo function_exists($name) ? "available\n" : "missing\n";
