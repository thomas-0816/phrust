<?php
// oracle-probe: id=oracle-builtin-contract-function-fpow-d5ad4010c2 area=builtin_contract kind=function symbol=fpow source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fpow-d5ad4010c2 failure_category=builtin_contract
$name = "fpow";
echo function_exists($name) ? "available\n" : "missing\n";
