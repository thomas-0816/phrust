<?php
// oracle-probe: id=oracle-builtin-contract-function-ob-get-length-6bbc174c25 area=builtin_contract kind=function symbol=ob_get_length source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ob-get-length-6bbc174c25 failure_category=builtin_contract
$name = "ob_get_length";
echo function_exists($name) ? "available\n" : "missing\n";
