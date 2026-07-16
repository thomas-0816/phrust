<?php
// oracle-probe: id=oracle-builtin-contract-function-ob-get-contents-9581ca7848 area=builtin_contract kind=function symbol=ob_get_contents source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ob-get-contents-9581ca7848 failure_category=builtin_contract
$name = "ob_get_contents";
echo function_exists($name) ? "available\n" : "missing\n";
