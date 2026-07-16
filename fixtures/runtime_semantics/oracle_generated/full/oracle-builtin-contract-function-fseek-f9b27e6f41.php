<?php
// oracle-probe: id=oracle-builtin-contract-function-fseek-f9b27e6f41 area=builtin_contract kind=function symbol=fseek source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fseek-f9b27e6f41 failure_category=builtin_contract
$name = "fseek";
echo function_exists($name) ? "available\n" : "missing\n";
