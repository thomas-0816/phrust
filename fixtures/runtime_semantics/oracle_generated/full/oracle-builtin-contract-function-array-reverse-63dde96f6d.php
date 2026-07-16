<?php
// oracle-probe: id=oracle-builtin-contract-function-array-reverse-63dde96f6d area=builtin_contract kind=function symbol=array_reverse source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-reverse-63dde96f6d failure_category=builtin_contract
$name = "array_reverse";
echo function_exists($name) ? "available\n" : "missing\n";
