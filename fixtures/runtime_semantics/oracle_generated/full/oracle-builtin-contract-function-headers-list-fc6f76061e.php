<?php
// oracle-probe: id=oracle-builtin-contract-function-headers-list-fc6f76061e area=builtin_contract kind=function symbol=headers_list source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-headers-list-fc6f76061e failure_category=builtin_contract
$name = "headers_list";
echo function_exists($name) ? "available\n" : "missing\n";
