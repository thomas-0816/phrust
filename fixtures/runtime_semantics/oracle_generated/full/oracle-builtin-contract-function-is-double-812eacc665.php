<?php
// oracle-probe: id=oracle-builtin-contract-function-is-double-812eacc665 area=builtin_contract kind=function symbol=is_double source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-double-812eacc665 failure_category=builtin_contract
$name = "is_double";
echo function_exists($name) ? "available\n" : "missing\n";
