<?php
// oracle-probe: id=oracle-builtin-contract-function-strcmp-e0e97d761c area=builtin_contract kind=function symbol=strcmp source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strcmp-e0e97d761c failure_category=builtin_contract
try {
    $result = \strcmp();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
