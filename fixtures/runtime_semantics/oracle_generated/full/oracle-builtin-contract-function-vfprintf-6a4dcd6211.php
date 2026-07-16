<?php
// oracle-probe: id=oracle-builtin-contract-function-vfprintf-6a4dcd6211 area=builtin_contract kind=function symbol=vfprintf source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-vfprintf-6a4dcd6211 failure_category=builtin_contract
$name = "vfprintf";
echo function_exists($name) ? "available\n" : "missing\n";
