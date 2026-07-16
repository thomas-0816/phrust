<?php
// oracle-probe: id=oracle-builtin-contract-function-is-object-9156bade3d area=builtin_contract kind=function symbol=is_object source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-object-9156bade3d failure_category=builtin_contract
$name = "is_object";
echo function_exists($name) ? "available\n" : "missing\n";
