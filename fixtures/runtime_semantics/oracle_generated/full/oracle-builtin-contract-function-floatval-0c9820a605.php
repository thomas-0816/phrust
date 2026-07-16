<?php
// oracle-probe: id=oracle-builtin-contract-function-floatval-0c9820a605 area=builtin_contract kind=function symbol=floatval source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-floatval-0c9820a605 failure_category=builtin_contract
$name = "floatval";
echo function_exists($name) ? "available\n" : "missing\n";
