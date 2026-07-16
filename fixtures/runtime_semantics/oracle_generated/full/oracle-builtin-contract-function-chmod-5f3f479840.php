<?php
// oracle-probe: id=oracle-builtin-contract-function-chmod-5f3f479840 area=builtin_contract kind=function symbol=chmod source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-chmod-5f3f479840 failure_category=builtin_contract
$name = "chmod";
echo function_exists($name) ? "available\n" : "missing\n";
