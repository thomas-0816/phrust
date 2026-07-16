<?php
// oracle-probe: id=oracle-builtin-contract-function-addslashes-64ff2e826f area=builtin_contract kind=function symbol=addslashes source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-addslashes-64ff2e826f failure_category=builtin_contract
$name = "addslashes";
echo function_exists($name) ? "available\n" : "missing\n";
