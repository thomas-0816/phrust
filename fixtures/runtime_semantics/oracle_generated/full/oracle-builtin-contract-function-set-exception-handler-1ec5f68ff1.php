<?php
// oracle-probe: id=oracle-builtin-contract-function-set-exception-handler-1ec5f68ff1 area=builtin_contract kind=function symbol=set_exception_handler source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-set-exception-handler-1ec5f68ff1 failure_category=builtin_contract
try {
    $result = \set_exception_handler();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
