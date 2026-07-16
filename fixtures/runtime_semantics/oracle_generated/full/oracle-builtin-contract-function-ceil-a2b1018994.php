<?php
// oracle-probe: id=oracle-builtin-contract-function-ceil-a2b1018994 area=builtin_contract kind=function symbol=ceil source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ceil-a2b1018994 failure_category=builtin_contract
$name = "ceil";
echo function_exists($name) ? "available\n" : "missing\n";
