<?php
// oracle-probe: id=oracle-builtin-contract-function-trigger-error-2c8671e7f9 area=builtin_contract kind=function symbol=trigger_error source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-trigger-error-2c8671e7f9 failure_category=builtin_contract
try {
    $result = \trigger_error();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
