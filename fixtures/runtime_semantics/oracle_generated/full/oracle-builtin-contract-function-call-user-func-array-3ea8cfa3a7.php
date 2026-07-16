<?php
// oracle-probe: id=oracle-builtin-contract-function-call-user-func-array-3ea8cfa3a7 area=builtin_contract kind=function symbol=call_user_func_array source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-call-user-func-array-3ea8cfa3a7 failure_category=builtin_contract
try {
    $result = \call_user_func_array();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
