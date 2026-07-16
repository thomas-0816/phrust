<?php
// oracle-probe: id=oracle-builtin-contract-function-ignore-user-abort-15aeb282c0 area=builtin_contract kind=function symbol=ignore_user_abort source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ignore-user-abort-15aeb282c0 failure_category=builtin_contract
$name = "ignore_user_abort";
echo function_exists($name) ? "available\n" : "missing\n";
