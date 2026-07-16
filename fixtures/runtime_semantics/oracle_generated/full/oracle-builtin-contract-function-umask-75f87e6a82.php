<?php
// oracle-probe: id=oracle-builtin-contract-function-umask-75f87e6a82 area=builtin_contract kind=function symbol=umask source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-umask-75f87e6a82 failure_category=builtin_contract
$name = "umask";
echo function_exists($name) ? "available\n" : "missing\n";
