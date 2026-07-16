<?php
// oracle-probe: id=oracle-builtin-contract-function-sinh-3883226cca area=builtin_contract kind=function symbol=sinh source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-sinh-3883226cca failure_category=builtin_contract
$name = "sinh";
echo function_exists($name) ? "available\n" : "missing\n";
