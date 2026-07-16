<?php
// oracle-probe: id=oracle-builtin-contract-function-closedir-386c5539db area=builtin_contract kind=function symbol=closedir source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-closedir-386c5539db failure_category=builtin_contract
$name = "closedir";
echo function_exists($name) ? "available\n" : "missing\n";
