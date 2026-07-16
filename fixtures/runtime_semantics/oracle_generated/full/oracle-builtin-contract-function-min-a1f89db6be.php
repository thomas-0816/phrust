<?php
// oracle-probe: id=oracle-builtin-contract-function-min-a1f89db6be area=builtin_contract kind=function symbol=min source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-min-a1f89db6be failure_category=builtin_contract
$name = "min";
echo function_exists($name) ? "available\n" : "missing\n";
