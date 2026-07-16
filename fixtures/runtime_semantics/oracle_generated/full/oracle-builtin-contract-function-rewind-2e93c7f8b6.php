<?php
// oracle-probe: id=oracle-builtin-contract-function-rewind-2e93c7f8b6 area=builtin_contract kind=function symbol=rewind source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-rewind-2e93c7f8b6 failure_category=builtin_contract
$name = "rewind";
echo function_exists($name) ? "available\n" : "missing\n";
