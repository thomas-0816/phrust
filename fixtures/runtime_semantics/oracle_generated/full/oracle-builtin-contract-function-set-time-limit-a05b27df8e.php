<?php
// oracle-probe: id=oracle-builtin-contract-function-set-time-limit-a05b27df8e area=builtin_contract kind=function symbol=set_time_limit source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-set-time-limit-a05b27df8e failure_category=builtin_contract
$name = "set_time_limit";
echo function_exists($name) ? "available\n" : "missing\n";
