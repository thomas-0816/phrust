<?php
// oracle-probe: id=oracle-builtin-contract-function-cosh-1ed6c9fa92 area=builtin_contract kind=function symbol=cosh source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-cosh-1ed6c9fa92 failure_category=builtin_contract
$name = "cosh";
echo function_exists($name) ? "available\n" : "missing\n";
