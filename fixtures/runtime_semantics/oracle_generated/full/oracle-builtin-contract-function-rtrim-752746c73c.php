<?php
// oracle-probe: id=oracle-builtin-contract-function-rtrim-752746c73c area=builtin_contract kind=function symbol=rtrim source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-rtrim-752746c73c failure_category=builtin_contract
$name = "rtrim";
echo function_exists($name) ? "available\n" : "missing\n";
