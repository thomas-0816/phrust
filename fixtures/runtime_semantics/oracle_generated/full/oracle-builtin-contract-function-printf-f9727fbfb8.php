<?php
// oracle-probe: id=oracle-builtin-contract-function-printf-f9727fbfb8 area=builtin_contract kind=function symbol=printf source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-printf-f9727fbfb8 failure_category=builtin_contract
$name = "printf";
echo function_exists($name) ? "available\n" : "missing\n";
