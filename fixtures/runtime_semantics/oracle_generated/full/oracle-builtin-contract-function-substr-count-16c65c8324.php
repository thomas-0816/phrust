<?php
// oracle-probe: id=oracle-builtin-contract-function-substr-count-16c65c8324 area=builtin_contract kind=function symbol=substr_count source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-substr-count-16c65c8324 failure_category=builtin_contract
$name = "substr_count";
echo function_exists($name) ? "available\n" : "missing\n";
