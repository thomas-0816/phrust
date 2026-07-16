<?php
// oracle-probe: id=oracle-builtin-contract-function-settype-8115ea361c area=builtin_contract kind=function symbol=settype source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-settype-8115ea361c failure_category=builtin_contract
$name = "settype";
echo function_exists($name) ? "available\n" : "missing\n";
