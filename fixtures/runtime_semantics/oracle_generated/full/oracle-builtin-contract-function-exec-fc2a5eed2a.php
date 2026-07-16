<?php
// oracle-probe: id=oracle-builtin-contract-function-exec-fc2a5eed2a area=builtin_contract kind=function symbol=exec source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-exec-fc2a5eed2a failure_category=builtin_contract
$name = "exec";
echo function_exists($name) ? "available\n" : "missing\n";
