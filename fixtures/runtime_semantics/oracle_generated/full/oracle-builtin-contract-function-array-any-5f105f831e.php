<?php
// oracle-probe: id=oracle-builtin-contract-function-array-any-5f105f831e area=builtin_contract kind=function symbol=array_any source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-any-5f105f831e failure_category=builtin_contract
$name = "array_any";
echo function_exists($name) ? "available\n" : "missing\n";
