<?php
// oracle-probe: id=oracle-builtin-contract-function-stripslashes-03c225238e area=builtin_contract kind=function symbol=stripslashes source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stripslashes-03c225238e failure_category=builtin_contract
$name = "stripslashes";
echo function_exists($name) ? "available\n" : "missing\n";
