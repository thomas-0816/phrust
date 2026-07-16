<?php
// oracle-probe: id=oracle-builtin-contract-function-func-get-arg-ab016a9b99 area=builtin_contract kind=function symbol=func_get_arg source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-func-get-arg-ab016a9b99 failure_category=builtin_contract
try {
    $result = \func_get_arg();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
