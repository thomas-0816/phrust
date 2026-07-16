<?php
// oracle-probe: id=oracle-builtin-contract-function-function-exists-d2f053595b area=builtin_contract kind=function symbol=function_exists source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-function-exists-d2f053595b failure_category=builtin_contract
try {
    $result = \function_exists();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
