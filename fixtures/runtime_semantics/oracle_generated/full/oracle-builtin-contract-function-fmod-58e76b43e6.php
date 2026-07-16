<?php
// oracle-probe: id=oracle-builtin-contract-function-fmod-58e76b43e6 area=builtin_contract kind=function symbol=fmod source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fmod-58e76b43e6 failure_category=builtin_contract
$name = "fmod";
echo function_exists($name) ? "available\n" : "missing\n";
