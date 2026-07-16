<?php
// oracle-probe: id=oracle-builtin-contract-function-assert-5377925b89 area=builtin_contract kind=function symbol=assert source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-assert-5377925b89 failure_category=builtin_contract
$name = "assert";
echo function_exists($name) ? "available\n" : "missing\n";
