<?php
// oracle-probe: id=oracle-builtin-contract-function-pi-90445f6ecb area=builtin_contract kind=function symbol=pi source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-pi-90445f6ecb failure_category=builtin_contract
$name = "pi";
echo function_exists($name) ? "available\n" : "missing\n";
