<?php
// oracle-probe: id=oracle-builtin-contract-function-chr-e1c3a3d700 area=builtin_contract kind=function symbol=chr source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-chr-e1c3a3d700 failure_category=builtin_contract
$name = "chr";
echo function_exists($name) ? "available\n" : "missing\n";
