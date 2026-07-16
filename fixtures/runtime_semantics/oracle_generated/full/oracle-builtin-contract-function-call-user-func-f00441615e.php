<?php
// oracle-probe: id=oracle-builtin-contract-function-call-user-func-f00441615e area=builtin_contract kind=function symbol=call_user_func source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-call-user-func-f00441615e failure_category=builtin_contract
$name = "call_user_func";
echo function_exists($name) ? "available\n" : "missing\n";
