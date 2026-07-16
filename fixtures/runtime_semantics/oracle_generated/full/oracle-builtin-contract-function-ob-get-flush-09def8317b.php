<?php
// oracle-probe: id=oracle-builtin-contract-function-ob-get-flush-09def8317b area=builtin_contract kind=function symbol=ob_get_flush source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ob-get-flush-09def8317b failure_category=builtin_contract
$name = "ob_get_flush";
echo function_exists($name) ? "available\n" : "missing\n";
