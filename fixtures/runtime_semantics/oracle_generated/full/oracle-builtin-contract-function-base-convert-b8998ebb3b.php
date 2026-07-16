<?php
// oracle-probe: id=oracle-builtin-contract-function-base-convert-b8998ebb3b area=builtin_contract kind=function symbol=base_convert source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-base-convert-b8998ebb3b failure_category=builtin_contract
$name = "base_convert";
echo function_exists($name) ? "available\n" : "missing\n";
