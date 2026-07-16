<?php
// oracle-probe: id=oracle-builtin-contract-function-strcspn-15ca0e43ca area=builtin_contract kind=function symbol=strcspn source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strcspn-15ca0e43ca failure_category=builtin_contract
$name = "strcspn";
echo function_exists($name) ? "available\n" : "missing\n";
