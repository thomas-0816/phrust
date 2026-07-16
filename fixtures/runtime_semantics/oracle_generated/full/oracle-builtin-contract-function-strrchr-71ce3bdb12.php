<?php
// oracle-probe: id=oracle-builtin-contract-function-strrchr-71ce3bdb12 area=builtin_contract kind=function symbol=strrchr source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strrchr-71ce3bdb12 failure_category=builtin_contract
$name = "strrchr";
echo function_exists($name) ? "available\n" : "missing\n";
