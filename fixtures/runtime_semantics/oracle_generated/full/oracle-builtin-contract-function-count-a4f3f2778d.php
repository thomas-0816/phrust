<?php
// oracle-probe: id=oracle-builtin-contract-function-count-a4f3f2778d area=builtin_contract kind=function symbol=count source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-count-a4f3f2778d failure_category=builtin_contract
$name = "count";
echo function_exists($name) ? "available\n" : "missing\n";
