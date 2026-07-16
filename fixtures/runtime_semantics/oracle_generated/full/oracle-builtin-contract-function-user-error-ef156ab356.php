<?php
// oracle-probe: id=oracle-builtin-contract-function-user-error-ef156ab356 area=builtin_contract kind=function symbol=user_error source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-user-error-ef156ab356 failure_category=builtin_contract
try {
    $result = \user_error();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
