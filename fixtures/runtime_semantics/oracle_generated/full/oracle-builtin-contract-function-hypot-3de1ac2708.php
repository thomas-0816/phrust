<?php
// oracle-probe: id=oracle-builtin-contract-function-hypot-3de1ac2708 area=builtin_contract kind=function symbol=hypot source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-hypot-3de1ac2708 failure_category=builtin_contract
$name = "hypot";
echo function_exists($name) ? "available\n" : "missing\n";
