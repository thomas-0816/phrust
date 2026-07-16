<?php
// oracle-probe: id=oracle-builtin-contract-function-strstr-9031e0e63b area=builtin_contract kind=function symbol=strstr source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strstr-9031e0e63b failure_category=builtin_contract
$name = "strstr";
echo function_exists($name) ? "available\n" : "missing\n";
