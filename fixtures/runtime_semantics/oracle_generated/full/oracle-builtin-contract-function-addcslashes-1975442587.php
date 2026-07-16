<?php
// oracle-probe: id=oracle-builtin-contract-function-addcslashes-1975442587 area=builtin_contract kind=function symbol=addcslashes source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-addcslashes-1975442587 failure_category=builtin_contract
$name = "addcslashes";
echo function_exists($name) ? "available\n" : "missing\n";
