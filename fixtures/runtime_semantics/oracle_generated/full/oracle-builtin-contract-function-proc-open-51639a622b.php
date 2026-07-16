<?php
// oracle-probe: id=oracle-builtin-contract-function-proc-open-51639a622b area=builtin_contract kind=function symbol=proc_open source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-proc-open-51639a622b failure_category=builtin_contract
$name = "proc_open";
echo function_exists($name) ? "available\n" : "missing\n";
