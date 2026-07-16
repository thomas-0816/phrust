<?php
// oracle-probe: id=oracle-builtin-contract-function-pack-5b5f796aea area=builtin_contract kind=function symbol=pack source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-pack-5b5f796aea failure_category=builtin_contract
$name = "pack";
echo function_exists($name) ? "available\n" : "missing\n";
