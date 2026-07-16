<?php
// oracle-probe: id=oracle-builtin-contract-function-fread-dd871385fe area=builtin_contract kind=function symbol=fread source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fread-dd871385fe failure_category=builtin_contract
$name = "fread";
echo function_exists($name) ? "available\n" : "missing\n";
