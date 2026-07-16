<?php
// oracle-probe: id=oracle-builtin-contract-function-unserialize-2b78628a5e area=builtin_contract kind=function symbol=unserialize source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-unserialize-2b78628a5e failure_category=builtin_contract
$name = "unserialize";
echo function_exists($name) ? "available\n" : "missing\n";
