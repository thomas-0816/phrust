<?php
// oracle-probe: id=oracle-builtin-contract-function-fwrite-0e2941b8d4 area=builtin_contract kind=function symbol=fwrite source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fwrite-0e2941b8d4 failure_category=builtin_contract
$name = "fwrite";
echo function_exists($name) ? "available\n" : "missing\n";
