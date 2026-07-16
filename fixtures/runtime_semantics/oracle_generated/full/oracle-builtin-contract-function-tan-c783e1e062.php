<?php
// oracle-probe: id=oracle-builtin-contract-function-tan-c783e1e062 area=builtin_contract kind=function symbol=tan source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-tan-c783e1e062 failure_category=builtin_contract
$name = "tan";
echo function_exists($name) ? "available\n" : "missing\n";
