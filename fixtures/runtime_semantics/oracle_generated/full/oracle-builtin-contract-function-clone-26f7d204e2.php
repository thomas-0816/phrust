<?php
// oracle-probe: id=oracle-builtin-contract-function-clone-26f7d204e2 area=builtin_contract kind=function symbol=clone source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-clone-26f7d204e2 failure_category=builtin_contract
try {
    $result = \clone();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
