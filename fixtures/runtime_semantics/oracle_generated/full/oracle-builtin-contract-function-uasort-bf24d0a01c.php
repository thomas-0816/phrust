<?php
// oracle-probe: id=oracle-builtin-contract-function-uasort-bf24d0a01c area=builtin_contract kind=function symbol=uasort source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-uasort-bf24d0a01c failure_category=builtin_contract
$name = "uasort";
echo function_exists($name) ? "available\n" : "missing\n";
