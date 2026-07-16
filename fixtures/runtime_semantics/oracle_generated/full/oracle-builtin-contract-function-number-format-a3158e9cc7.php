<?php
// oracle-probe: id=oracle-builtin-contract-function-number-format-a3158e9cc7 area=builtin_contract kind=function symbol=number_format source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-number-format-a3158e9cc7 failure_category=builtin_contract
$name = "number_format";
echo function_exists($name) ? "available\n" : "missing\n";
