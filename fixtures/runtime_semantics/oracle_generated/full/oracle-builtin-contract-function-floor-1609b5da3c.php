<?php
// oracle-probe: id=oracle-builtin-contract-function-floor-1609b5da3c area=builtin_contract kind=function symbol=floor source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-floor-1609b5da3c failure_category=builtin_contract
$name = "floor";
echo function_exists($name) ? "available\n" : "missing\n";
