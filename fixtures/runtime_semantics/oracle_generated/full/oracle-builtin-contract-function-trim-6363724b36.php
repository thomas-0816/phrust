<?php
// oracle-probe: id=oracle-builtin-contract-function-trim-6363724b36 area=builtin_contract kind=function symbol=trim source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-trim-6363724b36 failure_category=builtin_contract
$name = "trim";
echo function_exists($name) ? "available\n" : "missing\n";
