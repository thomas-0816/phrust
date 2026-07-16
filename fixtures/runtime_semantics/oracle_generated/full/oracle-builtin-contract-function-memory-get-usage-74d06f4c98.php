<?php
// oracle-probe: id=oracle-builtin-contract-function-memory-get-usage-74d06f4c98 area=builtin_contract kind=function symbol=memory_get_usage source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-memory-get-usage-74d06f4c98 failure_category=builtin_contract
$name = "memory_get_usage";
echo function_exists($name) ? "available\n" : "missing\n";
