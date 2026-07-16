<?php
// oracle-probe: id=oracle-builtin-contract-function-is-long-3d01aa621a area=builtin_contract kind=function symbol=is_long source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-long-3d01aa621a failure_category=builtin_contract
$name = "is_long";
echo function_exists($name) ? "available\n" : "missing\n";
