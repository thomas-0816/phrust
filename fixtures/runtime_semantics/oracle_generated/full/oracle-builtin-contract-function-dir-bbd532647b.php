<?php
// oracle-probe: id=oracle-builtin-contract-function-dir-bbd532647b area=builtin_contract kind=function symbol=dir source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-dir-bbd532647b failure_category=builtin_contract
$name = "dir";
echo function_exists($name) ? "available\n" : "missing\n";
