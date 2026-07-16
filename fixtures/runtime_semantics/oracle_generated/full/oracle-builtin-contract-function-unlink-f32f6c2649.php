<?php
// oracle-probe: id=oracle-builtin-contract-function-unlink-f32f6c2649 area=builtin_contract kind=function symbol=unlink source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-unlink-f32f6c2649 failure_category=builtin_contract
$name = "unlink";
echo function_exists($name) ? "available\n" : "missing\n";
