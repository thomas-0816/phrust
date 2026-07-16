<?php
// oracle-probe: id=oracle-builtin-contract-function-get-current-user-b9b81d563d area=builtin_contract kind=function symbol=get_current_user source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-current-user-b9b81d563d failure_category=builtin_contract
$name = "get_current_user";
echo function_exists($name) ? "available\n" : "missing\n";
