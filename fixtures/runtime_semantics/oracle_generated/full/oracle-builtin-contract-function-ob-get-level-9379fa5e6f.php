<?php
// oracle-probe: id=oracle-builtin-contract-function-ob-get-level-9379fa5e6f area=builtin_contract kind=function symbol=ob_get_level source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ob-get-level-9379fa5e6f failure_category=builtin_contract
$name = "ob_get_level";
echo function_exists($name) ? "available\n" : "missing\n";
