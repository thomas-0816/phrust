<?php
// oracle-probe: id=oracle-builtin-contract-function-setlocale-f7bb6b199b area=builtin_contract kind=function symbol=setlocale source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-setlocale-f7bb6b199b failure_category=builtin_contract
$name = "setlocale";
echo function_exists($name) ? "available\n" : "missing\n";
