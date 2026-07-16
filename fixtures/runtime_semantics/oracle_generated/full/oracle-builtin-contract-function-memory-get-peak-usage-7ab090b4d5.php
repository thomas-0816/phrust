<?php
// oracle-probe: id=oracle-builtin-contract-function-memory-get-peak-usage-7ab090b4d5 area=builtin_contract kind=function symbol=memory_get_peak_usage source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-memory-get-peak-usage-7ab090b4d5 failure_category=builtin_contract
$name = "memory_get_peak_usage";
echo function_exists($name) ? "available\n" : "missing\n";
